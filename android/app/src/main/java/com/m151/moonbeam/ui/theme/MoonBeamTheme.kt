package com.m151.moonbeam.ui.theme

import androidx.compose.material3.ExperimentalMaterial3ExpressiveApi
import androidx.compose.material3.MaterialExpressiveTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

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

@OptIn(ExperimentalMaterial3ExpressiveApi::class)
@Composable
fun MoonBeamTheme(content: @Composable () -> Unit) {
    MaterialExpressiveTheme(
        colorScheme = MoonBeamDarkColorScheme,
        content = content,
    )
}
