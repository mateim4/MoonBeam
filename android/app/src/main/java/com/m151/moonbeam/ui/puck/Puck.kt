package com.m151.moonbeam.ui.puck

import android.util.Log
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.VectorConverter
import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.spring
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.shadow
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.input.pointer.positionChange
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlin.math.roundToInt

@Composable
fun Puck(
    state: PuckState,
    onStateChange: (PuckState) -> Unit,
    containerWidth: Float,
    containerHeight: Float,
    modifier: Modifier = Modifier,
) {
    val density = LocalDensity.current
    val scope = rememberCoroutineScope()
    val baseSizeDp = 48.dp
    val baseSizePx = with(density) { baseSizeDp.toPx() }
    val radiusPx = baseSizePx / 2f

    // docs §2.2 shadow-grey #1B1725 @ 70%
    val surfaceColor = Color(0xFF1B1725).copy(alpha = 0.7f)

    val positionAnimatable = remember { Animatable(state.position, Offset.VectorConverter) }

    var lastInteractionTime by remember { mutableStateOf(System.currentTimeMillis()) }
    var isDragging by remember { mutableStateOf(false) }

    // Sync external position change to animatable
    LaunchedEffect(state.position) {
        if (!isDragging && positionAnimatable.value == Offset.Zero && state.position != Offset.Zero) {
            positionAnimatable.snapTo(state.position)
        }
    }

    // Drift-to-edge timer
    LaunchedEffect(lastInteractionTime, isDragging, state.anchored) {
        if (!isDragging && !state.anchored) {
            delay(3000)
            val currentPos = positionAnimatable.value
            val centerX = currentPos.x + radiusPx
            val centerY = currentPos.y + radiusPx

            val distLeft = centerX
            val distRight = containerWidth - centerX
            val distTop = centerY
            val distBottom = containerHeight - centerY

            val minDist = minOf(distLeft, distRight, distTop, distBottom)

            val targetPos = when (minDist) {
                distLeft -> Offset(-radiusPx * 0.5f, currentPos.y)
                distRight -> Offset(containerWidth - radiusPx * 1.5f, currentPos.y)
                distTop -> Offset(currentPos.x, -radiusPx * 0.5f)
                else -> Offset(currentPos.x, containerHeight - radiusPx * 1.5f)
            }

            onStateChange(state.copy(anchored = true, opacity = 0.25f))
            positionAnimatable.animateTo(
                targetPos,
                spring(dampingRatio = Spring.DampingRatioLowBouncy, stiffness = Spring.StiffnessLow)
            )
            onStateChange(state.copy(position = targetPos, anchored = true, opacity = 0.25f))
        }
    }

    val opacity by animateFloatAsState(targetValue = state.opacity, label = "opacity")
    val sizeMultiplier by animateFloatAsState(targetValue = state.sizeMultiplier, label = "size")

    // The root Box is the hit-test area. It always stays on screen and is 48dp.
    Box(
        modifier = modifier
            .offset {
                val visualPos = positionAnimatable.value
                val onScreenX = visualPos.x.coerceIn(0f, containerWidth - baseSizePx)
                val onScreenY = visualPos.y.coerceIn(0f, containerHeight - baseSizePx)
                IntOffset(onScreenX.roundToInt(), onScreenY.roundToInt())
            }
            .size(baseSizeDp)
            .pointerInput(containerWidth, containerHeight) {
                awaitEachGesture {
                    val down = awaitFirstDown(requireUnconsumed = false)
                    // Pen pass-through §4.4
                    if (down.type == PointerType.Stylus || down.type == PointerType.Eraser) {
                        return@awaitEachGesture
                    }

                    lastInteractionTime = System.currentTimeMillis()
                    val startPosition = positionAnimatable.value
                    var currentPosition = startPosition
                    var hasMovedBeyondSlop = false
                    val slopPx = with(density) { 10.dp.toPx() }

                    // Initial press feedback §4.2 PressIn
                    onStateChange(state.copy(sizeMultiplier = 1.166f, opacity = 0.9f))
                    isDragging = true

                    do {
                        val event = awaitPointerEvent()
                        val change = event.changes.first()

                        if (change.type == PointerType.Stylus || change.type == PointerType.Eraser) {
                            continue
                        }

                        if (change.pressed) {
                            val dragAmount = change.positionChange()
                            if (dragAmount != Offset.Zero) {
                                currentPosition += dragAmount
                                scope.launch {
                                    positionAnimatable.snapTo(currentPosition)
                                }
                                if ((currentPosition - startPosition).getDistance() > slopPx) {
                                    hasMovedBeyondSlop = true
                                }
                                change.consume()
                                lastInteractionTime = System.currentTimeMillis()
                            }
                        }
                    } while (event.changes.any { it.pressed })

                    isDragging = false
                    val endPosition = positionAnimatable.value

                    // Tap detection §10.3
                    if (!hasMovedBeyondSlop && (endPosition - startPosition).getDistance() < slopPx) {
                        Log.d("Puck", "tap")
                        // Spring back fully on-screen §4.3
                        val targetPos = Offset(
                            endPosition.x.coerceIn(0f, containerWidth - baseSizePx),
                            endPosition.y.coerceIn(0f, containerHeight - baseSizePx)
                        )
                        scope.launch {
                            positionAnimatable.animateTo(targetPos, spring(dampingRatio = Spring.DampingRatioLowBouncy, stiffness = Spring.StiffnessLow))
                            onStateChange(state.copy(position = targetPos, sizeMultiplier = 1.0f, opacity = 0.4f, anchored = false))
                        }
                    } else {
                        // Drag ended, check for stowage §4.5
                        val centerX = endPosition.x + radiusPx
                        val centerY = endPosition.y + radiusPx

                        val amountOffLeft = (radiusPx - centerX).coerceAtLeast(0f)
                        val amountOffRight = (centerX + radiusPx - containerWidth).coerceAtLeast(0f)
                        val amountOffTop = (radiusPx - centerY).coerceAtLeast(0f)
                        val amountOffBottom = (centerY + radiusPx - containerHeight).coerceAtLeast(0f)
                        val maxOff = maxOf(amountOffLeft, amountOffRight, amountOffTop, amountOffBottom)

                        if (maxOff > radiusPx * 1.5f) { // 75% of radius off-screen
                            val targetPos = when {
                                amountOffLeft > 0 -> Offset(-baseSizePx, endPosition.y)
                                amountOffRight > 0 -> Offset(containerWidth, endPosition.y)
                                amountOffTop > 0 -> Offset(endPosition.x, -baseSizePx)
                                else -> Offset(endPosition.x, containerHeight)
                            }
                            scope.launch {
                                positionAnimatable.snapTo(targetPos)
                                onStateChange(state.copy(position = targetPos, sizeMultiplier = 1.0f, opacity = 0.20f, anchored = true))
                            }
                        } else {
                            onStateChange(state.copy(position = endPosition, sizeMultiplier = 1.0f, opacity = 0.4f, anchored = false))
                        }
                    }
                    lastInteractionTime = System.currentTimeMillis()
                }
            }
    ) {
        val visualPos = positionAnimatable.value
        val isVisuallyStowed = visualPos.x <= -baseSizePx || visualPos.x >= containerWidth || visualPos.y <= -baseSizePx || visualPos.y >= containerHeight

        if (isVisuallyStowed) {
            // Tab indicator §4.5: 8dp wide, puck-height tall, 20% opacity
            val tabWidth = 8.dp
            val tabHeight = baseSizeDp
            Box(
                modifier = Modifier
                    .align(
                        when {
                            visualPos.x <= -baseSizePx -> Alignment.CenterStart
                            visualPos.x >= containerWidth -> Alignment.CenterEnd
                            visualPos.y <= -baseSizePx -> Alignment.TopCenter
                            else -> Alignment.BottomCenter
                        }
                    )
                    .size(
                        if (visualPos.x <= -baseSizePx || visualPos.x >= containerWidth) tabWidth else tabHeight,
                        if (visualPos.x <= -baseSizePx || visualPos.x >= containerWidth) tabHeight else tabWidth
                    )
                    .background(MaterialTheme.colorScheme.primary.copy(alpha = opacity), RoundedCornerShape(4.dp))
                    .semantics { contentDescription = "MoonBeam menu tab, tap to un-stow" }
            )
        } else {
            // The visual puck §4.1
            Box(
                modifier = Modifier
                    .offset {
                        val onScreenX = visualPos.x.coerceIn(0f, containerWidth - baseSizePx)
                        val onScreenY = visualPos.y.coerceIn(0f, containerHeight - baseSizePx)
                        IntOffset(
                            (visualPos.x - onScreenX).roundToInt(),
                            (visualPos.y - onScreenY).roundToInt()
                        )
                    }
                    .size(baseSizeDp * sizeMultiplier)
                    .graphicsLayer { this.alpha = opacity }
                    .shadow(4.dp, CircleShape)
                    .clip(CircleShape)
                    .background(surfaceColor)
                    .border(1.5.dp, MaterialTheme.colorScheme.primary, CircleShape),
                contentAlignment = Alignment.Center
            ) {
                Text(
                    text = "M",
                    color = MaterialTheme.colorScheme.primary,
                    fontSize = 16.sp
                )
            }
        }
    }
}
