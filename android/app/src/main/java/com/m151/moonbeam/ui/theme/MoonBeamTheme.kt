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

@Composable
fun MoonBeamTheme(content: @Composable () -> Unit) {
    // TODO(M4 phase 1+n): switch to MaterialExpressiveTheme once stable/public.
    // BOM 2025.06.00 has it as internal/ExperimentalMaterial3ExpressiveApi is not yet public.
    MaterialTheme(
        colorScheme = MoonBeamDarkColorScheme,
        content = content,
    )
}
