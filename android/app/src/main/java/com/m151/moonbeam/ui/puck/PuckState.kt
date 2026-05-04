package com.m151.moonbeam.ui.puck

import androidx.compose.ui.geometry.Offset

data class PuckState(
    val position: Offset = Offset.Zero,
    val anchored: Boolean = false,
    val opacity: Float = 0.4f,
    val sizeMultiplier: Float = 1.0f
)
