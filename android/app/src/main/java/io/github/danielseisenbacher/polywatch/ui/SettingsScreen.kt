package io.github.danielseisenbacher.polywatch.ui

import android.os.Build
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Visibility
import androidx.compose.material.icons.filled.VisibilityOff
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.danielseisenbacher.polywatch.BuildConfig
import io.github.danielseisenbacher.polywatch.data.AppViewModel
import io.github.danielseisenbacher.polywatch.data.Settings
import kotlinx.coroutines.launch

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun SettingsScreen(vm: AppViewModel, onBack: () -> Unit) {
    val s = vm.settings
    val go = LocalNav.current
    val scope = rememberCoroutineScope()
    val snackbar = remember { SnackbarHostState() }
    val dynamic by vm.dynamicColor.collectAsStateWithLifecycle()
    var owner by rememberSaveable { mutableStateOf(s.owner) }
    var repo by rememberSaveable { mutableStateOf(s.repo) }
    var data by rememberSaveable { mutableStateOf(s.dataOverride) }
    var token by rememberSaveable { mutableStateOf(s.token) }
    var showToken by remember { mutableStateOf(false) }

    PwScaffold(title = "Settings", onBack = onBack, snackbar = snackbar) { pad ->
        Column(
            Modifier
                .padding(pad)
                .imePadding()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 8.dp),
        ) {
            SectionTitle("Data source", sub = "The repository whose data branch and Actions runs the app shows")
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(owner, { owner = it }, Modifier.weight(1f), label = { Text("Owner") }, singleLine = true)
                OutlinedTextField(repo, { repo = it }, Modifier.weight(1.4f), label = { Text("Repository") }, singleLine = true)
            }
            Spacer(Modifier.height(8.dp))
            OutlinedTextField(
                data, { data = it }, Modifier.fillMaxWidth(),
                label = { Text("Data URL override (optional)") },
                placeholder = { Text("https://raw.githubusercontent.com/$owner/$repo/data/") },
                supportingText = { Text("Like the dashboard's ?data= parameter. Leave empty for the repo's data branch.") },
                singleLine = true,
            )

            SectionTitle("GitHub token", sub = "Optional. Lifts the API limit from 60 to 5,000 requests/hour and enables run, cancel and re-run.")
            OutlinedTextField(
                token, { token = it }, Modifier.fillMaxWidth(),
                label = { Text("Personal access token") },
                singleLine = true,
                visualTransformation = if (showToken) VisualTransformation.None else PasswordVisualTransformation(),
                trailingIcon = {
                    IconButton(onClick = { showToken = !showToken }) {
                        Icon(if (showToken) Icons.Filled.VisibilityOff else Icons.Filled.Visibility, if (showToken) "Hide token" else "Show token")
                    }
                },
                supportingText = {
                    Text("Use a fine-grained token limited to this repository with Actions: read and write. It is stored only on this device and excluded from backups.")
                },
            )
            TextButton(onClick = { go("https://github.com/settings/personal-access-tokens/new") }) { Text("Create a fine-grained token on GitHub") }

            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Button(onClick = {
                    val sourceChanged = owner.trim() != s.owner || repo.trim() != s.repo || data.trim() != s.dataOverride
                    s.owner = owner.ifBlank { Settings.DEFAULT_OWNER }
                    s.repo = repo.ifBlank { Settings.DEFAULT_REPO }
                    s.dataOverride = data
                    s.token = token
                    owner = s.owner; repo = s.repo
                    if (sourceChanged) vm.resetSource() else vm.refreshRuns()
                    scope.launch { snackbar.showSnackbar(if (sourceChanged) "Saved — reloading data" else "Saved") }
                }) { Text("Save") }
                OutlinedButton(onClick = {
                    owner = Settings.DEFAULT_OWNER; repo = Settings.DEFAULT_REPO; data = ""
                }) { Text("Reset source") }
            }

            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                SectionTitle("Appearance")
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text("Dynamic colour", style = MaterialTheme.typography.bodyLarge)
                        Text("Use colours from your wallpaper (Material You)", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    Spacer(Modifier.width(12.dp))
                    Switch(dynamic, vm::setDynamicColor)
                }
            }

            SectionTitle("Offline copy")
            Text(
                "The last downloaded data is kept on the device and shown when the network is unavailable.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            OutlinedButton(onClick = { vm.resetSource(); scope.launch { snackbar.showSnackbar("Cache cleared — reloading") } }, Modifier.padding(top = 8.dp)) {
                Text("Clear cache and reload")
            }

            HorizontalDivider(Modifier.padding(vertical = 20.dp))
            Text("polywatch ${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})", style = MaterialTheme.typography.titleSmall)
            Text(
                "Heuristic leads, not proof of insider trading. Scores flag unusual patterns; most have innocent explanations.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.padding(top = 8.dp)) {
                LinkChip("Repository", Links.repo(s))
                LinkChip("App releases", Links.repo(s) + "/releases/tag/apk-latest")
                LinkChip("Web dashboard", "https://${s.owner}.github.io/${s.repo}/")
            }
            Spacer(Modifier.height(24.dp))
        }
    }
}
