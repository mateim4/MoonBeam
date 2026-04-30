package com.m151.moonbeam.protocol

import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlinx.serialization.json.Json

/**
 * Constants and helpers for the binary wire format the host probes
 * locked: `[type:u8][flags:u8][payload]`.
 */
object Wire {
    const val TYPE_VIDEO: Byte = 0x01
    const val TYPE_AUDIO: Byte = 0x02
    const val TYPE_INPUT: Byte = 0x03

    // Round-trip latency measurement.
    //   PING — payload is an 8-byte little-endian u64 timestamp
    //          (microseconds, monotonic). Tablet picks the value.
    //   PONG — server echoes the exact same 8 bytes back with type
    //          swapped. No JSON, no parsing — minimum encode cost on
    //          the latency-critical path.
    const val TYPE_PING: Byte = 0x04
    const val TYPE_PONG: Byte = 0x05

    const val FLAG_NONE: Byte = 0x00
    const val FLAG_KEYFRAME: Byte = 0x01

    val json: Json = Json {
        ignoreUnknownKeys = true
        explicitNulls = false
        // Match Rust serde, which always emits every field. Without
        // this, default-valued fields (major=200, pressure=100, tilt=0)
        // would be omitted and the host would re-default them — fine
        // for behaviour today, but unnecessary protocol asymmetry.
        encodeDefaults = true
        classDiscriminator = "type"
    }

    /**
     * Wraps an [InputMsg] in a binary frame ready to send over the WS.
     */
    fun encodeInput(msg: InputMsg): ByteArray {
        val body = json.encodeToString(InputMsg.serializer(), msg).toByteArray(Charsets.UTF_8)
        val out = ByteArray(2 + body.size)
        out[0] = TYPE_INPUT
        out[1] = FLAG_NONE
        System.arraycopy(body, 0, out, 2, body.size)
        return out
    }

    /**
     * Build a ping frame carrying [timestampUs]. The server echoes
     * this back as PONG with the same payload.
     */
    fun encodePing(timestampUs: Long): ByteArray {
        val out = ByteArray(2 + 8)
        out[0] = TYPE_PING
        out[1] = FLAG_NONE
        ByteBuffer.wrap(out, 2, 8).order(ByteOrder.LITTLE_ENDIAN).putLong(timestampUs)
        return out
    }

    /**
     * Decodes a binary frame received from the host. Returns null for
     * non-input frames (the same /ws also carries video out).
     */
    fun decodeInbound(frame: ByteArray): Inbound? {
        if (frame.size < 2) return null
        val type = frame[0]
        val flags = frame[1]
        val payload = frame.copyOfRange(2, frame.size)
        return when (type) {
            TYPE_VIDEO -> Inbound.Video(payload, isKeyframe = (flags.toInt() and 0x01) != 0)
            TYPE_INPUT -> Inbound.Input(json.decodeFromString(InputMsg.serializer(), payload.toString(Charsets.UTF_8)))
            TYPE_PONG -> {
                if (payload.size < 8) null
                else Inbound.Pong(
                    timestampUs = ByteBuffer.wrap(payload).order(ByteOrder.LITTLE_ENDIAN).long,
                )
            }
            else -> null
        }
    }

    sealed class Inbound {
        data class Video(val annexB: ByteArray, val isKeyframe: Boolean) : Inbound() {
            override fun equals(other: Any?): Boolean =
                other is Video && annexB.contentEquals(other.annexB) && isKeyframe == other.isKeyframe
            override fun hashCode(): Int = annexB.contentHashCode() * 31 + isKeyframe.hashCode()
        }
        data class Input(val msg: InputMsg) : Inbound()
        data class Pong(val timestampUs: Long) : Inbound()
    }
}
