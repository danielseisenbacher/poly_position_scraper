//! Polygon JSON-RPC: block number and Deposit Wallet Factory logs.

use serde::Deserialize;
use serde_json::{json, Value};
use types::api::{ApiError, ApiResult, FactoryLog, RateClass};
use types::Addr;

use crate::http::excerpt;
use crate::Client;

#[derive(Deserialize)]
struct RpcResponse {
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
struct RpcError {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    message: String,
}

impl RpcError {
    /// The node refused the block span or result size (publicnode: -32701 "exceed maximum block range").
    fn is_range_limit(&self) -> bool {
        let m = self.message.to_ascii_lowercase();
        matches!(self.code, -32005 | -32701)
            || [
                "range",
                "too many",
                "limit exceeded",
                "exceed",
                "response size",
                "query returned more than",
            ]
            .iter()
            .any(|p| m.contains(p))
    }
}

enum CallError {
    Api(ApiError),
    Node(RpcError),
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawLog {
    #[serde(default)]
    topics: Vec<String>,
    #[serde(default)]
    block_number: String,
    #[serde(default)]
    transaction_hash: String,
    #[serde(default)]
    removed: bool,
}

fn parse_hex_u64(s: &str) -> Option<u64> {
    let t = s.trim();
    u64::from_str_radix(t.strip_prefix("0x").or_else(|| t.strip_prefix("0X"))?, 16).ok()
}

/// Decodes factory logs (topics[1] = proxy, topics[2] = owner); malformed rows are skipped.
fn decode_factory_logs(logs: Vec<RawLog>) -> Vec<FactoryLog> {
    logs.into_iter()
        .filter(|l| !l.removed && l.topics.len() >= 3)
        .filter_map(|l| {
            Some(FactoryLog {
                proxy: Addr::from_topic(&l.topics[1]).ok()?,
                owner: Addr::from_topic(&l.topics[2]).ok()?,
                block: parse_hex_u64(&l.block_number)?,
                tx: l.transaction_hash.to_ascii_lowercase(),
            })
        })
        .collect()
}

impl Client {
    async fn rpc_call(&self, method: &str, params: Value) -> Result<Value, CallError> {
        let url = self.cfg.rpc_url.clone();
        let body = json!({"jsonrpc": "2.0", "id": 1, "method": method, "params": params});
        let req = self.http.post(&url).json(&body).build().map_err(|e| {
            CallError::Api(ApiError::Transport {
                url: url.clone(),
                msg: e.to_string(),
            })
        })?;
        let resp: RpcResponse = self
            .get_json(RateClass::Rpc, req)
            .await
            .map_err(CallError::Api)?;
        match (resp.result, resp.error) {
            (_, Some(err)) => Err(CallError::Node(err)),
            (Some(v), None) => Ok(v),
            (None, None) => Err(CallError::Api(ApiError::Decode {
                url,
                msg: format!("{method}: neither result nor error"),
            })),
        }
    }

    fn node_error(&self, method: &str, e: RpcError) -> ApiError {
        self.count_error();
        ApiError::Http {
            status: 200,
            url: self.cfg.rpc_url.clone(),
            body: excerpt(&format!(
                "{method}: JSON-RPC error {}: {}",
                e.code, e.message
            )),
        }
    }

    pub(crate) async fn block_number_impl(&self) -> ApiResult<u64> {
        let v = match self.rpc_call("eth_blockNumber", json!([])).await {
            Ok(v) => v,
            Err(CallError::Api(e)) => return Err(e),
            Err(CallError::Node(e)) => return Err(self.node_error("eth_blockNumber", e)),
        };
        v.as_str().and_then(parse_hex_u64).ok_or_else(|| {
            self.count_error();
            ApiError::Decode {
                url: self.cfg.rpc_url.clone(),
                msg: format!(
                    "eth_blockNumber: unexpected result {}",
                    excerpt(&v.to_string())
                ),
            }
        })
    }

    /// Scans `[from_block, to_block]` in chunks of `rpc_log_chunk`, halving the chunk
    /// whenever the node rejects a span as too large.
    pub(crate) async fn factory_logs_impl(
        &self,
        from_block: u64,
        to_block: u64,
    ) -> ApiResult<Vec<FactoryLog>> {
        let mut out = Vec::new();
        let mut chunk = self.cfg.rpc_log_chunk.max(1);
        let mut from = from_block;
        while from <= to_block {
            let to = to_block.min(from.saturating_add(chunk - 1));
            let params = json!([{
                "fromBlock": format!("0x{from:x}"),
                "toBlock": format!("0x{to:x}"),
                "address": self.cfg.factory_address,
                "topics": [self.cfg.factory_topic0],
            }]);
            match self.rpc_call("eth_getLogs", params).await {
                Ok(v) => {
                    let logs: Vec<RawLog> = serde_json::from_value(v).map_err(|e| {
                        self.count_error();
                        ApiError::Decode {
                            url: self.cfg.rpc_url.clone(),
                            msg: format!("eth_getLogs: {e}"),
                        }
                    })?;
                    out.extend(decode_factory_logs(logs));
                    if to == u64::MAX {
                        break;
                    }
                    from = to + 1;
                }
                Err(CallError::Node(e)) if e.is_range_limit() && chunk > 1 => {
                    chunk = (chunk / 2).max(1);
                    tracing::debug!(chunk, error = %e.message, "eth_getLogs span rejected, halving");
                }
                Err(CallError::Node(e)) => return Err(self.node_error("eth_getLogs", e)),
                Err(CallError::Api(e)) => return Err(e),
            }
        }
        out.sort_by_key(|l| l.block);
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_and_range_errors() {
        assert_eq!(parse_hex_u64("0x5b79602"), Some(95_917_570));
        assert_eq!(parse_hex_u64("12"), None);
        assert!(RpcError {
            code: -32701,
            message: "exceed maximum block range: 10000".into()
        }
        .is_range_limit());
        assert!(RpcError {
            code: -32000,
            message: "query returned more than 10000 results".into()
        }
        .is_range_limit());
        assert!(!RpcError {
            code: -32000,
            message: "header not found".into()
        }
        .is_range_limit());
    }
}
