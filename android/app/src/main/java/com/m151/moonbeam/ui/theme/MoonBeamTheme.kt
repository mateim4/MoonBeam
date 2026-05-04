package com.m151.moonbeam.ui.theme

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable

private val MoonBeamDarkColorScheme = darkColorScheme(
    primary = MoonBeamColors.pearlAqua,
    onPrimary = MoonBeamColors.onPrimary,
    primaryContainer = MoonBeamColors.primaryContainer,
    onPrimaryContainer = MoonBeamColors.onPrimaryContainer,
    secondary = MoonBeamColors.softPeriwinkle,
    onSecondary = MoonBeamColors.shadowGrey,
    secondaryContainer = MoonBeamColors.secondaryContainer,
    onSecondaryContainer = MoonBeamColors.onSecondaryContainer,
    error = MoonBeamColors.vibrantCoral,
    onError = MoonBeamColors.parchment,
    errorContainer = MoonBeamColors.errorContainer,
    surface = MoonBeamColors.shadowGrey.copy(alpha = 0.7f),
    surfaceBright = MoonBeamColors.vintageGrape.copy(alpha = 0.85f),
    surfaceContainer = MoonBeamColors.vintageGrape.copy(alpha = 0.6f),
    onSurface = MoonBeamColors.parchment,
    onSurfaceVariant = MoonBeamColors.lavenderGrey,
    outlineVariant = MoonBeamColors.parchment.copy(alpha = 0.12f)
)

// TODO(M4 phase 1+n): switch to MaterialExpressiveTheme once stable.
// BOM 2025.06.00 ships MaterialExpressiveTheme as internal and does not
// expose @ExperimentalMaterial3ExpressiveApi yet; phase 2 (puck shape
// morph) is the natural moment to revisit when a later BOM publishes it.
@Composable
fun MoonBeamTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = MoonBeamDarkColorScheme,
        content = content,
    )
}
