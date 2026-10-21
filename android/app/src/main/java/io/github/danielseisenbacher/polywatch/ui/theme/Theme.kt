package io.github.danielseisenbacher.polywatch.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Typography
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.sp

/** Semantic colours from the dashboard's style.css, outside Material's role set. */
@Immutable
data class PwColors(
    val tierHigh: Color,
    val tierMedium: Color,
    val tierLow: Color,
    val pos: Color,
    val neg: Color,
    val buy: Color,
    val sell: Color,
    val redeem: Color,
    val warn: Color,
    val wonBg: Color,
    val lostBg: Color,
    val laneBg: Color,
    val bigBg: Color,
    val newBg: Color,
    val axis: Color,
    val signal: List<Color>,
) {
    fun tier(t: String?): Color = when (t) {
        "high" -> tierHigh
        "medium" -> tierMedium
        else -> tierLow
    }
    /** Signal/link palette, 1-based like the CSS `.s1`…`.s9` classes. */
    fun sig(c: Int): Color = signal[(c - 1).coerceIn(0, signal.lastIndex)]
}

private val DarkPw = PwColors(
    tierHigh = Color(0xFFD03B3B), tierMedium = Color(0xFFFAB219), tierLow = Color(0xFF6E6D68),
    pos = Color(0xFF3EC43E), neg = Color(0xFFEF6B6B),
    buy = Color(0xFF2FB52F), sell = Color(0xFFE66767), redeem = Color(0xFFB9A6FF), warn = Color(0xFFFAB219),
    wonBg = Color(0x210CA30C), lostBg = Color(0x21D03B3B), laneBg = Color(0x0AFFFFFF),
    bigBg = Color(0x14FAB219), newBg = Color(0x1A3987E5), axis = Color(0xFF45453F),
    signal = listOf(
        Color(0xFF3987E5), Color(0xFFD95926), Color(0xFF199E70), Color(0xFFC98500), Color(0xFFD55181),
        Color(0xFF2F9E2F), Color(0xFF9085E9), Color(0xFFE66767), Color(0xFF8A8A85),
    ),
)

private val LightPw = PwColors(
    tierHigh = Color(0xFFD03B3B), tierMedium = Color(0xFFFAB219), tierLow = Color(0xFFA6A59E),
    pos = Color(0xFF006300), neg = Color(0xFFB42323),
    buy = Color(0xFF0A8A0A), sell = Color(0xFFD03B3B), redeem = Color(0xFF6250D6), warn = Color(0xFF9A6200),
    wonBg = Color(0x1A0CA30C), lostBg = Color(0x17D03B3B), laneBg = Color(0x08000000),
    bigBg = Color(0x24FAB219), newBg = Color(0x142A78D6), axis = Color(0xFFB9B8AE),
    signal = listOf(
        Color(0xFF2A78D6), Color(0xFFEB6834), Color(0xFF1BAF7A), Color(0xFFEDA100), Color(0xFFE87BA4),
        Color(0xFF008300), Color(0xFF6250D6), Color(0xFFE34948), Color(0xFF898781),
    ),
)

// Fallback schemes (Android < 12 or dynamic colour off), seeded from the dashboard accent #3987E5.
private val LightScheme = lightColorScheme(
    primary = Color(0xFF2A5EA7), onPrimary = Color.White,
    primaryContainer = Color(0xFFD6E3FF), onPrimaryContainer = Color(0xFF001B3E),
    secondary = Color(0xFF555F71), onSecondary = Color.White,
    secondaryContainer = Color(0xFFD9E3F8), onSecondaryContainer = Color(0xFF121C2B),
    tertiary = Color(0xFF6F5576), onTertiary = Color.White,
    tertiaryContainer = Color(0xFFF9D8FF), onTertiaryContainer = Color(0xFF28132F),
    error = Color(0xFFBA1A1A), errorContainer = Color(0xFFFFDAD6), onErrorContainer = Color(0xFF410002),
    background = Color(0xFFF9F9FF), onBackground = Color(0xFF1A1C20),
    surface = Color(0xFFF9F9FF), onSurface = Color(0xFF1A1C20),
    surfaceVariant = Color(0xFFE0E2EC), onSurfaceVariant = Color(0xFF44474E),
    outline = Color(0xFF74777F), outlineVariant = Color(0xFFC4C6D0),
    surfaceContainerLowest = Color.White, surfaceContainerLow = Color(0xFFF3F3FA),
    surfaceContainer = Color(0xFFEDEDF4), surfaceContainerHigh = Color(0xFFE7E8EE),
    surfaceContainerHighest = Color(0xFFE2E2E9),
)

private val DarkScheme = darkColorScheme(
    primary = Color(0xFFA9C7FF), onPrimary = Color(0xFF003063),
    primaryContainer = Color(0xFF00468C), onPrimaryContainer = Color(0xFFD6E3FF),
    secondary = Color(0xFFBDC7DC), onSecondary = Color(0xFF273141),
    secondaryContainer = Color(0xFF3E4758), onSecondaryContainer = Color(0xFFD9E3F8),
    tertiary = Color(0xFFDCBCE1), onTertiary = Color(0xFF3E2845),
    tertiaryContainer = Color(0xFF563E5D), onTertiaryContainer = Color(0xFFF9D8FF),
    error = Color(0xFFFFB4AB), errorContainer = Color(0xFF93000A), onErrorContainer = Color(0xFFFFDAD6),
    background = Color(0xFF111318), onBackground = Color(0xFFE2E2E9),
    surface = Color(0xFF111318), onSurface = Color(0xFFE2E2E9),
    surfaceVariant = Color(0xFF44474E), onSurfaceVariant = Color(0xFFC4C6D0),
    outline = Color(0xFF8E9099), outlineVariant = Color(0xFF44474E),
    surfaceContainerLowest = Color(0xFF0C0E13), surfaceContainerLow = Color(0xFF1A1C20),
    surfaceContainer = Color(0xFF1E2025), surfaceContainerHigh = Color(0xFF282A2F),
    surfaceContainerHighest = Color(0xFF33353A),
)

val LocalPw = staticCompositionLocalOf { LightPw }

val Mono = FontFamily.Monospace

private val base = Typography()
private val AppTypography = base.copy(
    labelSmall = base.labelSmall.copy(fontSize = 11.sp),
)

/** Tabular figures for numbers in lists, like the dashboard's `.num` cells. */
val NumStyle = TextStyle(fontFeatureSettings = "tnum")

@Composable
fun PolywatchTheme(dynamicColor: Boolean = true, content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    val ctx = LocalContext.current
    val scheme = when {
        dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ->
            if (dark) dynamicDarkColorScheme(ctx) else dynamicLightColorScheme(ctx)
        dark -> DarkScheme
        else -> LightScheme
    }
    CompositionLocalProvider(LocalPw provides if (dark) DarkPw else LightPw) {
        MaterialTheme(colorScheme = scheme, typography = AppTypography, content = content)
    }
}

object Pw {
    val colors: PwColors
        @Composable get() = LocalPw.current
}
