package com.m151.moonbeam.ui.radial

import android.util.Log
import androidx.activity.compose.BackHandler
import androidx.compose.animation.core.*
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.*
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.scale
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import kotlinx.coroutines.delay
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.roundToInt
import kotlin.math.sin

@Composable
fun RadialMenu(
    state: RadialMenuState,
    containerWidth: Float,
    containerHeight: Float,
    onDismiss: () -> Unit,
    onModeToggle: () -> Unit,
    onQualityToggle: () -> Unit,
    onAudioToggle: () -> Unit,
    onRotate: () -> Unit,
    onSettings: () -> Unit,
    onDisconnect: () -> Unit
) {
    if (!state.isOpen) return

    val density = LocalDensity.current

    // Animation stage 1: Puck dilation §5.3
    val puckScale by animateFloatAsState(
        targetValue = if (state.isOpen) 80f / 48f else 1f,
        animationSpec = spring(stiffness = Spring.StiffnessMedium),
        label = "puck_dilation"
    )

    // Animation stage 2: Items emergence §5.3
    var showItems by remember { mutableStateOf(false) }
    LaunchedEffect(state.isOpen) {
        if (state.isOpen) {
            delay(150)
            showItems = true
        } else {
            showItems = false
        }
    }

    // Inactivity timeout §5.5
    var lastInteraction by remember { mutableStateOf(System.currentTimeMillis()) }
    LaunchedEffect(lastInteraction, state.isOpen) {
        if (state.isOpen) {
            delay(4000)
            onDismiss()
        }
    }

    BackHandler(onBack = onDismiss)

    Box(
        modifier = Modifier
            .fillMaxSize()
            .pointerInput(Unit) {
                // Tap outside dismiss §5.5 + pen pass-through §4.4
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = true)
                    if (down.type == PointerType.Stylus || down.type == PointerType.Eraser) {
                        return@awaitEachGesture
                    }
                    onDismiss()
                }
            }
    ) {
        // Morphing puck background
        Box(
            modifier = Modifier
                .offset {
                    IntOffset(
                        (state.center.x - with(density) { 24.dp.toPx() }).roundToInt(),
                        (state.center.y - with(density) { 24.dp.toPx() }).roundToInt()
                    )
                }
                .size(48.dp)
                .scale(puckScale)
                .clip(CircleShape)
                .background(Color(0xFF1B1725).copy(alpha = 0.7f)) // shadow-grey
        )

        val itemData = listOf(
            RadialItemDef(
                icon = if (state.mode == RadialMode.EXTEND) Icons.Filled.ScreenShare else Icons.Filled.Tv,
                label = if (state.mode == RadialMode.EXTEND) "Extend mode" else "Mirror mode",
                onClick = {
                    Log.d("Radial", if (state.mode == RadialMode.EXTEND) "Mirror mode" else "Extend mode")
                    onModeToggle()
                    lastInteraction = System.currentTimeMillis()
                }
            ),
            RadialItemDef(
                icon = if (state.quality == RadialQuality.DRAWING) Icons.Filled.Edit else Icons.Filled.Visibility,
                label = if (state.quality == RadialQuality.DRAWING) "Drawing mode" else "Display mode",
                onClick = {
                    Log.d("Radial", if (state.quality == RadialQuality.DRAWING) "Display mode" else "Drawing mode")
                    onQualityToggle()
                    lastInteraction = System.currentTimeMillis()
                }
            ),
            RadialItemDef(
                icon = Icons.Filled.ScreenRotation,
                label = "Rotate",
                onClick = {
                    Log.d("Radial", "Rotate")
                    onRotate()
                    onDismiss()
                }
            ),
            RadialItemDef(
                icon = if (state.audio == RadialAudio.ON) Icons.Filled.VolumeUp else Icons.Filled.VolumeOff,
                label = if (state.audio == RadialAudio.ON) "Audio on" else "Audio off",
                onClick = {
                    Log.d("Radial", if (state.audio == RadialAudio.ON) "Audio off" else "Audio on")
                    onAudioToggle()
                    lastInteraction = System.currentTimeMillis()
                }
            ),
            RadialItemDef(
                icon = Icons.Filled.Settings,
                label = "Settings",
                onClick = {
                    Log.d("Radial", "Settings")
                    onSettings()
                    onDismiss()
                }
            ),
            RadialItemDef(
                icon = Icons.Filled.PowerSettingsNew,
                label = "Disconnect",
                onClick = {
                    Log.d("Radial", "Disconnect")
                    onDisconnect()
                    onDismiss()
                }
            )
        )

        // Edge-aware rotation §5.2
        val itemRadiusPx = with(density) { 88.dp.toPx() }
        val itemMarginPx = with(density) { 28.dp.toPx() }

        val baseAngle = remember(state.center, containerWidth, containerHeight) {
            var best = -90f
            var minOffScreen = 7

            // Try different rotations to see which one fits best
            for (offset in listOf(0f, 30f, -30f, 60f, -60f, 90f, -90f, 180f)) {
                val candidate = -90f + offset
                var offScreenCount = 0
                for (i in 0 until 6) {
                    val angle = candidate + i * 60f
                    val rad = angle * PI.toFloat() / 180f
                    val x = state.center.x + itemRadiusPx * cos(rad)
                    val y = state.center.y + itemRadiusPx * sin(rad)
                    if (x < itemMarginPx || x > containerWidth - itemMarginPx ||
                        y < itemMarginPx || y > containerHeight - itemMarginPx) {
                        offScreenCount++
                    }
                }
                if (offScreenCount < minOffScreen) {
                    minOffScreen = offScreenCount
                    best = candidate
                }
                if (minOffScreen == 0) break
            }
            best
        }

        itemData.forEachIndexed { index, item ->
            val angleDeg = baseAngle + (index * 60f)
            val angleRad = angleDeg * PI.toFloat() / 180f

            val itemX = state.center.x + itemRadiusPx * cos(angleRad)
            val itemY = state.center.y + itemRadiusPx * sin(angleRad)

            // Staggered item entry §5.3
            var itemTriggered by remember { mutableStateOf(false) }
            LaunchedEffect(showItems) {
                if (showItems) {
                    delay(index * 30L)
                    itemTriggered = true
                } else {
                    itemTriggered = false
                }
            }

            val itemScale by animateFloatAsState(
                targetValue = if (itemTriggered) 1f else 0f,
                animationSpec = spring(
                    dampingRatio = Spring.DampingRatioMediumBouncy,
                    stiffness = Spring.StiffnessLow
                ),
                label = "item_scale_$index"
            )

            if (itemScale > 0.01f) {
                RadialItem(
                    icon = item.icon,
                    contentDescription = item.label,
                    onClick = item.onClick,
                    modifier = Modifier
                        .offset {
                            IntOffset(
                                (itemX - with(density) { 28.dp.toPx() }).roundToInt(),
                                (itemY - with(density) { 28.dp.toPx() }).roundToInt()
                            )
                        }
                        .scale(itemScale)
                )
            }
        }
    }
}

private data class RadialItemDef(
    val icon: ImageVector,
    val label: String,
    val onClick: () -> Unit
)
