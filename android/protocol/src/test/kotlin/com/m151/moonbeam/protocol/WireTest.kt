package com.m151.moonbeam.protocol

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * Round-trip tests for the wire encoding. The Rust host expects the
 * exact JSON shape these tests pin: the byte-level diff between
 * Kotlin and Rust serialisers is what would cause a silent
 * "events arrive but nothing draws" failure in M3.
 */
class WireTest {
    @Test
    fun `pen_down encodes with type tag and snake_case fields`() {
        val msg = InputMsg.PenDown(x = 100, y = 200, pressure = 2048, tiltX = -3, tiltY = 1)
        val bytes = Wire.encodeInput(msg)
        // [0x03][0x00][json...]
        assertEquals(Wire.TYPE_INPUT, bytes[0])
        assertEquals(Wire.FLAG_NONE, bytes[1])
        val json = bytes.copyOfRange(2, bytes.size).toString(Charsets.UTF_8)
        assertTrue("\"type\":\"pen_down\"" in json) { json }
        assertTrue("\"tilt_x\":-3" in json) { json }
        assertTrue("\"tilt_y\":1" in json) { json }
    }

    @Test
    fun `pen_up encodes as just the discriminator`() {
        val bytes = Wire.encodeInput(InputMsg.PenUp)
        val json = bytes.copyOfRange(2, bytes.size).toString(Charsets.UTF_8)
        assertEquals("""{"type":"pen_up"}""", json)
    }

    @Test
    fun `touch_down round-trips defaults`() {
        val msg = InputMsg.TouchDown(slot = 0, id = 1001, x = 1480, y = 924)
        val bytes = Wire.encodeInput(msg)
        val json = bytes.copyOfRange(2, bytes.size).toString(Charsets.UTF_8)
        assertTrue("\"major\":200" in json) { json }
        assertTrue("\"pressure\":100" in json) { json }
    }

    @Test
    fun `pen_button uses lowercase enum`() {
        val bytes = Wire.encodeInput(InputMsg.PenButton(PenButtonId.STYLUS, true))
        val json = bytes.copyOfRange(2, bytes.size).toString(Charsets.UTF_8)
        assertTrue("\"button\":\"stylus\"" in json) { json }
    }
}
