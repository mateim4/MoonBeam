package com.m151.moonbeam.ui.radial

import androidx.compose.ui.geometry.Offset

data class RadialMenuState(
    val isOpen: Boolean = false,
    val center: Offset = Offset.Zero,
    val mode: RadialMode = RadialMode.EXTEND,
    val quality: RadialQuality = RadialQuality.DISPLAY,
    val audio: RadialAudio = RadialAudio.ON
)

enum class RadialMode { EXTEND, MIRROR }
enum class RadialQuality { DRAWING, DISPLAY }
enum class RadialAudio { ON, OFF }
