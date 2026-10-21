// GitHub Actions: the pipeline's workflow runs, their jobs, and (with a token) run/cancel/re-run.
package io.github.danielseisenbacher.polywatch.ui

import androidx.compose.animation.animateContentSize
import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.OpenInNew
import androidx.compose.material.icons.filled.Block
import androidx.compose.material.icons.filled.Cancel
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.ExpandLess
import androidx.compose.material.icons.filled.ExpandMore
import androidx.compose.material.icons.filled.Key
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.filled.RemoveCircleOutline
import androidx.compose.material.icons.filled.Replay
import androidx.compose.material.icons.filled.Schedule
import androidx.compose.material.icons.filled.StopCircle
import androidx.compose.material.icons.filled.TimerOff
import androidx.compose.material.icons.outlined.Circle
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import io.github.danielseisenbacher.polywatch.data.AppViewModel
import io.github.danielseisenbacher.polywatch.data.DispatchInput
import io.github.danielseisenbacher.polywatch.data.Job
import io.github.danielseisenbacher.polywatch.data.Workflow
import io.github.danielseisenbacher.polywatch.data.WorkflowRun
import io.github.danielseisenbacher.polywatch.data.isoToEpoch
import io.github.danielseisenbacher.polywatch.ui.theme.Pw
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@Composable
fun RunStatusIcon(status: String, conclusion: String?, size: Int = 24) {
    val m = Modifier.size(size.dp)
    val pw = Pw.colors
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    when {
        status == "in_progress" -> CircularProgressIndicator(Modifier.size((size - 4).dp), strokeWidth = 2.5.dp)
        status != "completed" -> Icon(Icons.Filled.Schedule, status, m, tint = pw.warn)
        conclusion == "success" -> Icon(Icons.Filled.CheckCircle, "success", m, tint = pw.pos)
        conclusion == "failure" || conclusion == "startup_failure" -> Icon(Icons.Filled.Cancel, conclusion, m, tint = pw.neg)
        conclusion == "cancelled" -> Icon(Icons.Filled.Block, "cancelled", m, tint = muted)
        conclusion == "skipped" -> Icon(Icons.Filled.RemoveCircleOutline, "skipped", m, tint = muted)
        conclusion == "timed_out" -> Icon(Icons.Filled.TimerOff, "timed out", m, tint = pw.neg)
        else -> Icon(Icons.Outlined.Circle, conclusion ?: status, m, tint = muted)
    }
}

private fun statusText(status: String, conclusion: String?) =
    if (status == "completed") (conclusion ?: "completed").replace('_', ' ') else status.replace('_', ' ')

/** Poll [block] every [periodMs] while the screen is at least STARTED. */
@Composable
private fun PollWhileVisible(key: Any?, periodMs: Long, block: suspend () -> Unit) {
    val lifecycle = LocalLifecycleOwner.current.lifecycle
    LaunchedEffect(key, periodMs) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            while (true) {
                delay(periodMs)
                block()
            }
        }
    }
}

@Composable
fun RunsScreen(vm: AppViewModel) {
    val rs by vm.runs.collectAsStateWithLifecycle()
    val go = LocalNav.current
    val now = LocalNow.current
    val hasToken = vm.settings.token.isNotBlank()
    var dispatch by remember { mutableStateOf(false) }
    var needToken by remember { mutableStateOf(false) }
    val snackbar = remember { SnackbarHostState() }

    LaunchedEffect(Unit) {
        if (rs.loadedAt == 0L || System.currentTimeMillis() - rs.loadedAt > 30_000) vm.refreshRuns()
    }
    // Unauthenticated API: 60 requests/hour, so poll gently without a token.
    val active = rs.runs.any { it.active }
    PollWhileVisible(active to hasToken, if (active) (if (hasToken) 15_000 else 60_000) else (if (hasToken) 60_000 else 300_000)) { vm.refreshRuns() }

    PwScaffold(
        title = "Actions",
        subtitle = "${vm.settings.owner}/${vm.settings.repo}" + (vm.github.rateRemaining?.let { " · API $it/${vm.github.rateLimit ?: "?"} left" } ?: ""),
        refreshing = rs.loading,
        onRefresh = { vm.refreshRuns() },
        snackbar = snackbar,
        actions = { IconButton(onClick = { go(Links.repo(vm.settings) + "/actions") }) { Icon(Icons.AutoMirrored.Filled.OpenInNew, "Open on GitHub") } },
        fab = {
            ExtendedFloatingActionButton(
                onClick = { if (hasToken) dispatch = true else needToken = true },
                icon = { Icon(Icons.Filled.PlayArrow, null) },
                text = { Text("Run workflow") },
            )
        },
    ) { pad ->
        LazyColumn(contentPadding = PaddingValues(start = 12.dp, end = 12.dp, top = 4.dp, bottom = pad.calculateBottomPadding() + 96.dp)) {
            if (!hasToken) item {
                Card(
                    Modifier.fillMaxWidth().padding(horizontal = 4.dp, vertical = 4.dp),
                    colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.tertiaryContainer),
                ) {
                    Row(Modifier.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                        Icon(Icons.Filled.Key, null)
                        Spacer(Modifier.width(12.dp))
                        Text(
                            "Read-only without sign-in (GitHub allows 60 requests/hour). Add a token to run, cancel or re-run workflows.",
                            Modifier.weight(1f),
                            style = MaterialTheme.typography.bodySmall,
                        )
                        TextButton(onClick = { go("settings") }) { Text("Add") }
                    }
                }
            }
            item {
                Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 4.dp, vertical = 4.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilterChip(rs.filter == null, { vm.refreshRuns(null) }, label = { Text("All") })
                    rs.workflows.filter { it.state == "active" }.sortedBy { it.name }.forEach { wf ->
                        FilterChip(rs.filter == wf.id, { vm.refreshRuns(if (rs.filter == wf.id) null else wf.id) }, label = { Text(wf.name) })
                    }
                }
            }
            rs.err?.let { e -> item { ErrorCard(e, Modifier.padding(4.dp), onRetry = { vm.refreshRuns() }) } }
            if (rs.runs.isEmpty() && rs.loading) item { LoadingBox("runs") }
            if (rs.runs.isEmpty() && !rs.loading && rs.err == null) item { EmptyText("No runs", Modifier.padding(horizontal = 4.dp)) }
            items(rs.runs, key = { it.id }) { r -> RunItem(r, now) { go("run/${r.id}") } }
        }
    }

    if (needToken) AlertDialog(
        onDismissRequest = { needToken = false },
        icon = { Icon(Icons.Filled.Key, null) },
        title = { Text("GitHub token needed") },
        text = { Text("Starting workflows needs a GitHub token with Actions: read and write on ${vm.settings.owner}/${vm.settings.repo}. Add one in Settings.") },
        confirmButton = { TextButton(onClick = { needToken = false; go("settings") }) { Text("Settings") } },
        dismissButton = { TextButton(onClick = { needToken = false }) { Text("Cancel") } },
    )
    if (dispatch) DispatchSheet(vm, rs.workflows.filter { it.state == "active" }, onDismiss = { dispatch = false }) { msg ->
        dispatch = false
        vm.refreshRuns()
        snackbar.showSnackbar(msg)
        delay(4000)
        vm.refreshRuns()
    }
}

@Composable
private fun RunItem(r: WorkflowRun, now: Long, onClick: () -> Unit) {
    ListItem(
        modifier = Modifier.clickable(onClick = onClick),
        colors = ListItemDefaults.colors(containerColor = Color.Transparent),
        leadingContent = { RunStatusIcon(r.status, r.conclusion) },
        headlineContent = { Text(r.displayTitle.ifBlank { r.name }, maxLines = 2, overflow = TextOverflow.Ellipsis) },
        supportingContent = {
            Text(
                listOfNotNull(r.name, "#${r.runNumber}", r.event.replace('_', ' '), r.headBranch).joinToString(" · "),
                maxLines = 1, overflow = TextOverflow.Ellipsis,
            )
        },
        trailingContent = {
            Column(horizontalAlignment = Alignment.End) {
                Text(relShort(r.createdTs, now), style = MaterialTheme.typography.labelMedium)
                Text(duration(r.durationS(now)), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        },
    )
}

// ---- run detail -----------------------------------------------------------------------

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun RunDetailScreen(vm: AppViewModel, id: Long, onBack: () -> Unit) {
    val go = LocalNav.current
    val now = LocalNow.current
    val scope = rememberCoroutineScope()
    val snackbar = remember { SnackbarHostState() }
    val hasToken = vm.settings.token.isNotBlank()
    var run by remember { mutableStateOf<WorkflowRun?>(null) }
    var jobs by remember { mutableStateOf<List<Job>>(emptyList()) }
    var err by remember { mutableStateOf<String?>(null) }
    var loading by remember { mutableStateOf(true) }
    var reload by remember { mutableIntStateOf(0) }
    var busy by remember { mutableStateOf(false) }
    var confirmCancel by remember { mutableStateOf(false) }

    suspend fun load() {
        loading = true
        try {
            run = vm.github.run(id)
            jobs = vm.github.jobs(id)
            err = null
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            err = e.message ?: e.toString()
        }
        loading = false
    }
    LaunchedEffect(id, reload) { load() }
    val active = run?.active == true
    if (active) PollWhileVisible(id, if (hasToken) 10_000 else 45_000) { load() }

    fun act(label: String, f: suspend () -> Unit) {
        scope.launch {
            busy = true
            try {
                f()
                snackbar.showSnackbar("$label requested")
                delay(2500)
                reload++
                vm.refreshRuns()
            } catch (e: CancellationException) {
                throw e
            } catch (e: Exception) {
                snackbar.showSnackbar(e.message ?: e.toString())
            } finally {
                busy = false
            }
        }
    }

    PwScaffold(
        title = run?.let { it.displayTitle.ifBlank { it.name } } ?: "Run $id",
        subtitle = run?.let { "${it.name} #${it.runNumber}" + if (it.runAttempt > 1) " · attempt ${it.runAttempt}" else "" },
        onBack = onBack,
        refreshing = loading && run != null,
        onRefresh = { reload++ },
        snackbar = snackbar,
        actions = { IconButton(onClick = { reload++ }) { Icon(Icons.Filled.Refresh, "Refresh") } },
    ) { pad ->
        LazyColumn(contentPadding = PaddingValues(start = 16.dp, end = 16.dp, top = 4.dp, bottom = pad.calculateBottomPadding() + 24.dp)) {
            err?.let { e -> item { ErrorCard(e, Modifier.padding(bottom = 8.dp), onRetry = { reload++ }) } }
            val r = run
            if (r == null) {
                if (err == null) item { LoadingBox("run") }
                return@LazyColumn
            }
            item {
                Card(Modifier.fillMaxWidth(), colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow)) {
                    Column(Modifier.padding(16.dp)) {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            RunStatusIcon(r.status, r.conclusion, 28)
                            Spacer(Modifier.width(12.dp))
                            Text(statusText(r.status, r.conclusion).replaceFirstChar { it.uppercase() }, style = MaterialTheme.typography.titleLarge)
                        }
                        Spacer(Modifier.height(8.dp))
                        Text("${r.event.replace('_', ' ')} on ${r.headBranch ?: "?"} · ${r.headSha.take(7)}", style = MaterialTheme.typography.bodyMedium)
                        (r.triggeringActor ?: r.actor)?.login?.let { Text("by $it", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                        Text(
                            "started ${rel(r.startedTs, now)} (${date(r.startedTs, true)}) · ${if (r.active) "running" else "took"} ${duration(r.durationS(now))}",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                        Spacer(Modifier.height(12.dp))
                        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                            FilledTonalButton(onClick = { go(r.htmlUrl) }) {
                                Icon(Icons.AutoMirrored.Filled.OpenInNew, null, Modifier.size(18.dp))
                                Spacer(Modifier.width(6.dp))
                                Text("Logs on GitHub")
                            }
                            if (r.active) OutlinedButton(onClick = { confirmCancel = true }, enabled = hasToken && !busy) {
                                Icon(Icons.Filled.StopCircle, null, Modifier.size(18.dp))
                                Spacer(Modifier.width(6.dp))
                                Text("Cancel")
                            } else {
                                OutlinedButton(onClick = { act("Re-run") { vm.github.rerun(r.id, failedOnly = false) } }, enabled = hasToken && !busy) {
                                    Icon(Icons.Filled.Replay, null, Modifier.size(18.dp))
                                    Spacer(Modifier.width(6.dp))
                                    Text("Re-run all")
                                }
                                if (r.conclusion == "failure") OutlinedButton(onClick = { act("Re-run of failed jobs") { vm.github.rerun(r.id, failedOnly = true) } }, enabled = hasToken && !busy) {
                                    Text("Re-run failed")
                                }
                            }
                        }
                        if (!hasToken) Text(
                            "Add a GitHub token in Settings to cancel or re-run.",
                            Modifier.padding(top = 8.dp),
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
            item { SectionTitle("Jobs", sub = "${jobs.size} job${if (jobs.size == 1) "" else "s"}") }
            if (jobs.isEmpty() && !loading) item { EmptyText("No jobs yet") }
            items(jobs, key = { it.id }) { j -> JobCard(j, now) }
        }
    }

    if (confirmCancel) AlertDialog(
        onDismissRequest = { confirmCancel = false },
        title = { Text("Cancel this run?") },
        text = { Text("${run?.name} #${run?.runNumber} will stop. Partial results of a data-writing job are not committed.") },
        confirmButton = { TextButton(onClick = { confirmCancel = false; act("Cancel") { vm.github.cancel(id) } }) { Text("Cancel run") } },
        dismissButton = { TextButton(onClick = { confirmCancel = false }) { Text("Keep running") } },
    )
}

@Composable
private fun JobCard(j: Job, now: Long) {
    val go = LocalNav.current
    var open by remember(j.id) { mutableStateOf(j.status != "completed" || j.conclusion == "failure") }
    val start = isoToEpoch(j.startedAt)
    val end = if (j.completedAt != null) isoToEpoch(j.completedAt) else now
    Card(
        Modifier.fillMaxWidth().padding(bottom = 8.dp).animateContentSize(),
        colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surfaceContainerLow),
    ) {
        Row(Modifier.clickable { open = !open }.padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
            RunStatusIcon(j.status, j.conclusion)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(j.name, style = MaterialTheme.typography.titleSmall)
                Text(
                    statusText(j.status, j.conclusion) + if (start > 0) " · ${duration((end - start).coerceAtLeast(0))}" else "",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            IconButton(onClick = { go(j.htmlUrl) }) { Icon(Icons.AutoMirrored.Filled.OpenInNew, "Open job log on GitHub") }
            Icon(if (open) Icons.Filled.ExpandLess else Icons.Filled.ExpandMore, if (open) "Collapse" else "Expand")
        }
        if (open) {
            HorizontalDivider()
            Column(Modifier.padding(vertical = 4.dp)) {
                j.steps.forEach { s ->
                    val ss = isoToEpoch(s.startedAt)
                    val se = if (s.completedAt != null) isoToEpoch(s.completedAt) else now
                    Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
                        RunStatusIcon(s.status, s.conclusion, 18)
                        Spacer(Modifier.width(10.dp))
                        Text(s.name, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        if (ss > 0 && s.status != "queued") Text(duration((se - ss).coerceAtLeast(0)), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                if (j.steps.isEmpty()) EmptyText("No steps reported yet", Modifier.padding(horizontal = 12.dp))
            }
        }
    }
}

// ---- dispatch -------------------------------------------------------------------------

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DispatchSheet(vm: AppViewModel, workflows: List<Workflow>, onDismiss: () -> Unit, onDone: suspend (String) -> Unit) {
    val scope = rememberCoroutineScope()
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var wf by remember { mutableStateOf<Workflow?>(null) }
    var ref by remember { mutableStateOf("main") }
    var inputs by remember { mutableStateOf<List<DispatchInput>?>(null) }
    val values = remember { mutableStateMapOf<String, String>() }
    var loading by remember { mutableStateOf(false) }
    var err by remember { mutableStateOf<String?>(null) }
    var notDispatchable by remember { mutableStateOf(false) }

    LaunchedEffect(wf, ref) {
        val w = wf ?: return@LaunchedEffect
        loading = true; err = null; notDispatchable = false; inputs = null
        try {
            val ins = vm.github.dispatchInputs(w, ref)
            if (ins == null) notDispatchable = true
            else {
                inputs = ins
                values.clear()
                ins.forEach { values[it.name] = it.default.ifEmpty { if (it.type == "boolean") "false" else it.options.firstOrNull() ?: "" } }
            }
        } catch (e: CancellationException) {
            throw e
        } catch (e: Exception) {
            err = "Could not read ${w.path}@$ref: ${e.message}"
        }
        loading = false
    }

    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = state) {
        Column(
            Modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 24.dp).padding(bottom = 24.dp).navigationBarsPadding().imePadding(),
        ) {
            Text("Run workflow", style = MaterialTheme.typography.titleLarge)
            Spacer(Modifier.height(12.dp))
            val w = wf
            if (w == null) {
                Text("Choose a workflow", style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
                if (workflows.isEmpty()) EmptyText("No workflows loaded — pull to refresh the Actions tab first.")
                workflows.sortedBy { it.name }.forEach { x ->
                    ListItem(
                        headlineContent = { Text(x.name) },
                        supportingContent = { Text(x.path) },
                        leadingContent = { Icon(Icons.Filled.PlayArrow, null) },
                        colors = ListItemDefaults.colors(containerColor = Color.Transparent),
                        modifier = Modifier.clickable { wf = x },
                    )
                }
                return@Column
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(w.name, style = MaterialTheme.typography.titleMedium)
                    Text(w.path, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                TextButton(onClick = { wf = null }) { Text("Change") }
            }
            Spacer(Modifier.height(8.dp))
            OutlinedTextField(ref, { ref = it.trim() }, Modifier.fillMaxWidth(), label = { Text("Branch") }, singleLine = true)
            Spacer(Modifier.height(12.dp))
            when {
                loading -> Box(Modifier.fillMaxWidth().padding(16.dp), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
                err != null -> ErrorCard(err!!)
                notDispatchable -> ErrorCard("${w.name} has no manual trigger (workflow_dispatch), so it cannot be started from here.")
                inputs != null -> {
                    inputs!!.forEach { inp -> InputField(inp, values[inp.name] ?: "") { values[inp.name] = it } }
                    if (inputs!!.isEmpty()) Text("No inputs.", style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Spacer(Modifier.height(16.dp))
                    Button(
                        onClick = {
                            scope.launch {
                                loading = true
                                try {
                                    vm.github.dispatch(w, ref, values.toMap())
                                    onDone("Started ${w.name} on $ref — it shows up in a few seconds")
                                } catch (e: CancellationException) {
                                    throw e
                                } catch (e: Exception) {
                                    err = e.message ?: e.toString()
                                    loading = false
                                }
                            }
                        },
                        enabled = ref.isNotBlank() && inputs!!.none { it.required && values[it.name].isNullOrBlank() },
                        modifier = Modifier.fillMaxWidth(),
                    ) {
                        Icon(Icons.Filled.PlayArrow, null)
                        Spacer(Modifier.width(8.dp))
                        Text("Run ${w.name}", fontWeight = FontWeight.SemiBold)
                    }
                }
            }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun InputField(inp: DispatchInput, value: String, onChange: (String) -> Unit) {
    Column(Modifier.padding(vertical = 6.dp)) {
        when {
            inp.type == "boolean" -> Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(inp.name, style = MaterialTheme.typography.titleSmall)
                    if (inp.description.isNotBlank()) Text(inp.description, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
                Spacer(Modifier.width(12.dp))
                Switch(value == "true", { onChange(it.toString()) })
            }
            inp.options.isNotEmpty() -> {
                Text(inp.name, style = MaterialTheme.typography.titleSmall)
                if (inp.description.isNotBlank()) Text(inp.description, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    inp.options.forEach { o -> FilterChip(value == o, { onChange(o) }, label = { Text(o) }) }
                }
            }
            else -> OutlinedTextField(
                value, onChange, Modifier.fillMaxWidth(),
                label = { Text(inp.name + if (inp.required) " *" else "") },
                supportingText = if (inp.description.isNotBlank()) ({ Text(inp.description) }) else null,
                singleLine = true,
            )
        }
    }
}
