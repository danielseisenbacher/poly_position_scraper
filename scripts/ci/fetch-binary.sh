#!/usr/bin/env bash
# Download the polywatch binary that build.yml publishes on release `bin-latest`
# into bin/polywatch and verify its checksum.
# Env: GH_TOKEN, GITHUB_REPOSITORY.
set -euo pipefail

repo="${GITHUB_REPOSITORY:?GITHUB_REPOSITORY is not set}"
mkdir -p bin
gh release download bin-latest -R "$repo" -p polywatch -p polywatch.sha256 -D bin --clobber
(cd bin && sha256sum --check --strict polywatch.sha256)
chmod +x bin/polywatch
bin/polywatch --version
