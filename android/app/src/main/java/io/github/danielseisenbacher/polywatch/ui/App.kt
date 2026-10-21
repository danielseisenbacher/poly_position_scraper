package io.github.danielseisenbacher.polywatch.ui

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Insights
import androidx.compose.material.icons.filled.PersonSearch
import androidx.compose.material.icons.filled.RocketLaunch
import androidx.compose.material.icons.filled.SwapVert
import androidx.compose.material.icons.outlined.Insights
import androidx.compose.material.icons.outlined.PersonSearch
import androidx.compose.material.icons.outlined.RocketLaunch
import androidx.compose.material.icons.outlined.SwapVert
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.pulltorefresh.PullToRefreshBox
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.nestedscroll.nestedScroll
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LocalLifecycleOwner
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.repeatOnLifecycle
import androidx.navigation.NavGraph.Companion.findStartDestination
import androidx.navigation.NavHostController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.currentBackStackEntryAsState
import androidx.navigation.compose.rememberNavController
import io.github.danielseisenbacher.polywatch.data.AppViewModel
import io.github.danielseisenbacher.polywatch.data.nowS
import kotlinx.coroutines.delay

/** Wall clock in unix seconds, ticking every 30 s so "x min ago" labels stay current. */
val LocalNow = compositionLocalOf { nowS() }

/** Navigate to an in-app route, or open an http(s) URL in a Custom Tab. */
val LocalNav = compositionLocalOf<(String) -> Unit> { {} }

private const val REFRESH_MS = 5 * 60 * 1000L

private data class Tab(val route: String, val label: String, val icon: ImageVector, val selectedIcon: ImageVector)

private val TABS = listOf(
    Tab("overview", "Candidates", Icons.Outlined.PersonSearch, Icons.Filled.PersonSearch),
    Tab("tape", "Tape", Icons.Outlined.SwapVert, Icons.Filled.SwapVert),
    Tab("stats", "Stats", Icons.Outlined.Insights, Icons.Filled.Insights),
    Tab("runs", "Actions", Icons.Outlined.RocketLaunch, Icons.Filled.RocketLaunch),
)

@Composable
fun App(vm: AppViewModel) {
    val nav = rememberNavController()
    val ctx = LocalContext.current
    var now by remember { mutableLongStateOf(nowS()) }
    val lifecycle = LocalLifecycleOwner.current.lifecycle

    // Like the dashboard: re-poll meta/index/tape every 5 minutes while visible.
    LaunchedEffect(lifecycle) {
        lifecycle.repeatOnLifecycle(Lifecycle.State.STARTED) {
            while (true) {
                now = nowS()
                val c = vm.core.value
                if (!c.loading && c.loadedAt > 0 && System.currentTimeMillis() - c.loadedAt > REFRESH_MS) vm.autoRefresh()
                delay(30_000)
            }
        }
    }

    val go: (String) -> Unit = { route ->
        if (route.startsWith("http")) openUrl(ctx, route) else nav.navigate(route) { launchSingleTop = true }
    }
    val entry by nav.currentBackStackEntryAsState()
    val currentRoute = entry?.destination?.route
    val showBar = TABS.any { it.route == currentRoute }

    CompositionLocalProvider(LocalNow provides now, LocalNav provides go) {
        Scaffold(
            contentWindowInsets = WindowInsets(0),
            bottomBar = {
                if (showBar) NavigationBar {
                    TABS.forEach { t ->
                        val sel = currentRoute == t.route
                        NavigationBarItem(
                            selected = sel,
                            onClick = { switchTab(nav, t.route) },
                            icon = { Icon(if (sel) t.selectedIcon else t.icon, null) },
                            label = { Text(t.label) },
                        )
                    }
                }
            },
        ) { inner ->
            NavHost(nav, startDestination = "overview", modifier = Modifier.padding(inner)) {
                composable("overview") { OverviewScreen(vm) }
                composable("tape") { TapeScreen(vm) }
                composable("stats") { StatsScreen(vm, onFilterSignal = { switchTab(nav, "overview") }) }
                composable("runs") { RunsScreen(vm) }
                composable("wallet/{addr}") { WalletScreen(vm, it.arguments?.getString("addr").orEmpty(), nav::popBackStack) }
                composable("market/{id}") { MarketScreen(vm, it.arguments?.getString("id").orEmpty(), nav::popBackStack) }
                composable("cluster/{id}") { ClusterScreen(vm, it.arguments?.getString("id").orEmpty(), nav::popBackStack) }
                composable("run/{id}") { RunDetailScreen(vm, it.arguments?.getString("id")?.toLongOrNull() ?: 0, nav::popBackStack) }
                composable("settings") { SettingsScreen(vm, nav::popBackStack) }
            }
        }
    }
}

private fun switchTab(nav: NavHostController, route: String) {
    nav.navigate(route) {
        popUpTo(nav.graph.findStartDestination().id) { saveState = true }
        launchSingleTop = true
        restoreState = true
    }
}

/**
 * Screen scaffold: top app bar (with back arrow on detail screens) and optional pull-to-refresh.
 * Top-level screens sit above the bottom bar, so they leave the bottom inset to it.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PwScaffold(
    title: String,
    subtitle: String? = null,
    onBack: (() -> Unit)? = null,
    refreshing: Boolean? = null,
    onRefresh: () -> Unit = {},
    snackbar: SnackbarHostState? = null,
    actions: @Composable RowScope.() -> Unit = {},
    fab: @Composable () -> Unit = {},
    content: @Composable (PaddingValues) -> Unit,
) {
    val scroll = TopAppBarDefaults.pinnedScrollBehavior()
    Scaffold(
        modifier = Modifier.nestedScroll(scroll.nestedScrollConnection),
        contentWindowInsets = if (onBack == null) WindowInsets(0) else WindowInsets.navigationBars,
        snackbarHost = { if (snackbar != null) SnackbarHost(snackbar) },
        floatingActionButton = fab,
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text(title, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        if (subtitle != null) Text(
                            subtitle,
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                },
                navigationIcon = {
                    if (onBack != null) IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, "Back") }
                },
                actions = actions,
                scrollBehavior = scroll,
            )
        },
    ) { inner ->
        if (refreshing != null) {
            PullToRefreshBox(refreshing, onRefresh, Modifier.fillMaxSize().padding(top = inner.calculateTopPadding())) {
                content(PaddingValues(bottom = inner.calculateBottomPadding()))
            }
        } else {
            content(inner)
        }
    }
}

@Composable
fun rememberCore(vm: AppViewModel) = vm.core.collectAsStateWithLifecycle()
