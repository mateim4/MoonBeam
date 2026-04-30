package com.m151.moonbeam.input

import android.util.Log
import android.view.MotionEvent
import com.m151.moonbeam.protocol.InputMsg
import com.m151.moonbeam.protocol.PenButtonId
import kotlin.math.cos
import kotlin.math.sin

/**
 * Translates Android [MotionEvent]s into [InputMsg]s sent over the
 * WebSocket.
 *
 * Pen vs touch is dispatched on the per-pointer ToolType. Coordinates
 * are passed through 1:1 — the assumption (per
 * MOONBEAM-APP-PLAN.md §4) is that the tablet runs at native
 * resolution matching the host's virtual display, so the SurfaceView's
 * pixel space and the uinput device's coordinate space are the same.
 *
 * Multi-touch slot allocation: Android `pointerId`s are reused across
 * gestures but stable within one. We use the Android pointer id
 * directly as the uinput MT slot. Tracking ids are issued per-stroke
 * (not reused) so libinput can't dedupe two real touches.
 */
class TouchHandler(
    private val send: (InputMsg, eventTimeMs: Long) -> Unit,
    private val pressureMax: Int = 4095,
) {
    private val activeSlots = IntArray(MAX_SLOTS) { -1 } // slot -> tracking_id (or -1 if free)
    private var nextTrackingId = 1000
    private var lastButtonState = 0
    private var penInContact = false

    /**
     * Returns true if the event was consumed.
     */
    fun handle(event: MotionEvent): Boolean {
        val idx = event.actionIndex.coerceIn(0, event.pointerCount - 1)
        val toolType = event.getToolType(idx)
        if (event.actionMasked == MotionEvent.ACTION_DOWN ||
            event.actionMasked == MotionEvent.ACTION_POINTER_DOWN
        ) {
            Log.d("MoonBeam.Touch", "tool=$toolType pressure=${event.getPressure(idx)}")
        }
        return when (toolType) {
            MotionEvent.TOOL_TYPE_STYLUS, MotionEvent.TOOL_TYPE_ERASER -> handlePen(event)
            else -> handleTouch(event)
        }
    }

    private fun handlePen(event: MotionEvent): Boolean {
        val i = 0 // S-Pen is single-pointer; index 0 always
        val x = event.getX(i).toInt()
        val y = event.getY(i).toInt()
        val pressure = (event.getPressure(i) * pressureMax)
            .toInt()
            .coerceIn(0, pressureMax)
        val tiltRad = event.getAxisValue(MotionEvent.AXIS_TILT, i)
        val orientationRad = event.getOrientation(i)
        // Decompose Android's polar-style tilt (tilt magnitude 0..π/2,
        // orientation -π..+π) into Cartesian tiltX/tiltY in degrees.
        // Caps at ±90° matching the uinput device's range.
        val sinTilt = sin(tiltRad)
        val tiltX = (sinTilt * cos(orientationRad) * 90.0).toInt().coerceIn(-90, 90)
        val tiltY = (sinTilt * sin(orientationRad) * 90.0).toInt().coerceIn(-90, 90)

        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                send(InputMsg.PenDown(x, y, pressure.coerceAtLeast(1), tiltX, tiltY), event.eventTime)
                penInContact = true
            }
            MotionEvent.ACTION_MOVE -> {
                if (penInContact) {
                    send(InputMsg.PenMove(x, y, pressure.coerceAtLeast(1), tiltX, tiltY), event.eventTime)
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> {
                if (penInContact) {
                    send(InputMsg.PenUp, event.eventTime)
                    penInContact = false
                }
            }
            // Hover events come through onGenericMotionEvent, not here.
        }

        // Stylus button transitions are reported via `buttonState`. We
        // diff against last seen so we send one event per transition,
        // not one per move.
        val buttonState = event.buttonState
        val changed = buttonState xor lastButtonState
        if (changed and MotionEvent.BUTTON_STYLUS_PRIMARY != 0) {
            val pressed = (buttonState and MotionEvent.BUTTON_STYLUS_PRIMARY) != 0
            send(InputMsg.PenButton(PenButtonId.STYLUS, pressed), event.eventTime)
        }
        if (changed and MotionEvent.BUTTON_STYLUS_SECONDARY != 0) {
            val pressed = (buttonState and MotionEvent.BUTTON_STYLUS_SECONDARY) != 0
            send(InputMsg.PenButton(PenButtonId.STYLUS2, pressed), event.eventTime)
        }
        lastButtonState = buttonState

        return true
    }

    private fun handleTouch(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN, MotionEvent.ACTION_POINTER_DOWN -> {
                val idx = event.actionIndex
                val slot = event.getPointerId(idx).coerceIn(0, MAX_SLOTS - 1)
                val tid = nextTrackingId++
                activeSlots[slot] = tid
                send(InputMsg.TouchDown(
                    slot = slot,
                    id = tid,
                    x = event.getX(idx).toInt(),
                    y = event.getY(idx).toInt(),
                ), event.eventTime)
            }
            MotionEvent.ACTION_MOVE -> {
                for (idx in 0 until event.pointerCount) {
                    val slot = event.getPointerId(idx).coerceIn(0, MAX_SLOTS - 1)
                    if (activeSlots[slot] < 0) continue
                    send(InputMsg.TouchMove(
                        slot = slot,
                        x = event.getX(idx).toInt(),
                        y = event.getY(idx).toInt(),
                    ), event.eventTime)
                }
            }
            MotionEvent.ACTION_UP, MotionEvent.ACTION_POINTER_UP -> {
                val idx = event.actionIndex
                val slot = event.getPointerId(idx).coerceIn(0, MAX_SLOTS - 1)
                if (activeSlots[slot] >= 0) {
                    send(InputMsg.TouchUp(slot), event.eventTime)
                    activeSlots[slot] = -1
                }
            }
            MotionEvent.ACTION_CANCEL -> {
                // Release every active slot — the OS is yanking the
                // gesture away (e.g. system gesture interception).
                for (slot in activeSlots.indices) {
                    if (activeSlots[slot] >= 0) {
                        send(InputMsg.TouchUp(slot), event.eventTime)
                        activeSlots[slot] = -1
                    }
                }
            }
        }
        return true
    }

    companion object {
        private const val MAX_SLOTS = 10
    }
}
