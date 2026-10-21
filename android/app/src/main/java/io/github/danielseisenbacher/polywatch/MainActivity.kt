package io.github.danielseisenbacher.polywatch

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.runtime.getValue
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.danielseisenbacher.polywatch.data.AppViewModel
import io.github.danielseisenbacher.polywatch.ui.App
import io.github.danielseisenbacher.polywatch.ui.theme.PolywatchTheme

class MainActivity : ComponentActivity() {
    private val vm: AppViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        setContent {
            val dynamic by vm.dynamicColor.collectAsStateWithLifecycle()
            PolywatchTheme(dynamicColor = dynamic) {
                App(vm)
            }
        }
    }
}
