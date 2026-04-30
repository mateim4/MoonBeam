package com.m151.moonbeam.protocol

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable

/**
 * Kotlin twin of the Rust-side `InputMsg` (host/src/bin/probe-input-server.rs).
 *
 * Wire format (locked in M2 step 3): the WS payload after the 2-byte
 * [type=0x03][flags] header is a UTF-8 JSON object whose `"type"` field
 * names one of the variants below.
 *
 * Both ends use serde-/kotlinx-style tagged unions so the serializers
 * agree byte-for-byte without hand-written codecs.
 */
@Serializable
sealed class InputMsg {
    @Serializable
    @SerialName("pen_down")
    data class PenDown(
        val x: Int,
        val y: Int,
        val pressure: Int,
        @SerialName("tilt_x") val tiltX: Int = 0,
        @SerialName("tilt_y") val tiltY: Int = 0,
    ) : InputMsg()

    @Serializable
    @SerialName("pen_move")
    data class PenMove(
        val x: Int,
        val y: Int,
        val pressure: Int,
        @SerialName("tilt_x") val tiltX: Int = 0,
        @SerialName("tilt_y") val tiltY: Int = 0,
    ) : InputMsg()

    @Serializable
    @SerialName("pen_up")
    data object PenUp : InputMsg()

    @Serializable
    @SerialName("pen_button")
    data class PenButton(
        val button: PenButtonId,
        val state: Boolean,
    ) : InputMsg()

    @Serializable
    @SerialName("touch_down")
    data class TouchDown(
        val slot: Int,
        val id: Int,
        val x: Int,
        val y: Int,
        val major: Int = 200,
        val pressure: Int = 100,
    ) : InputMsg()

    @Serializable
    @SerialName("touch_move")
    data class TouchMove(
        val slot: Int,
        val x: Int,
        val y: Int,
        val major: Int = 200,
        val pressure: Int = 100,
    ) : InputMsg()

    @Serializable
    @SerialName("touch_up")
    data class TouchUp(
        val slot: Int,
    ) : InputMsg()
}

@Serializable
enum class PenButtonId {
    @SerialName("stylus")
    STYLUS,

    @SerialName("stylus2")
    STYLUS2,
}
