package com.m151.moonbeam.decode

import android.media.MediaCodec
import android.media.MediaCodec.BufferInfo
import android.media.MediaFormat
import android.util.Log
import android.view.Surface

/**
 * Hardware H.264 decoder fed Annex-B access units from the WS,
 * outputting decoded frames directly to the [outputSurface].
 *
 * Threading: the [feed] / [drain] methods can be called from any
 * thread, but **do not** call them concurrently — there's exactly one
 * MediaCodec input pump. M3 step 2 calls them from a single coroutine
 * inside the activity scope.
 *
 * Why we don't pre-configure with SPS/PPS: NVENC's `repeat_pps`
 * setting (M1 step 3) prepends SPS+PPS to every IDR. MediaCodec
 * accepts an Annex-B stream with in-band parameter sets and discovers
 * the real width/height from the first IDR — the configure() values
 * here are placeholders, just satisfying the API.
 */
class VideoDecoder(
    private val outputSurface: Surface,
    private val placeholderWidth: Int = 1920,
    private val placeholderHeight: Int = 1080,
) {
    private var codec: MediaCodec? = null
    private var started = false
    private val bufferInfo = BufferInfo()

    fun start() {
        if (started) return
        val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, placeholderWidth, placeholderHeight)
        // Hint the decoder to favour latency over throughput. On Pixel
        // and Samsung devices this maps to the "low-latency" tunnel
        // that skips reordering buffers.
        format.setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
        // NVENC writes baseline-friendly streams; some decoders refuse
        // unless we declare the profile. We omit it and let MediaCodec
        // discover from the in-stream SPS — works on every Android
        // hardware decoder we'd ship to.

        val codec = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
        codec.configure(format, outputSurface, null, 0)
        codec.start()
        this.codec = codec
        started = true
        Log.i(TAG, "MediaCodec started (placeholder ${placeholderWidth}x${placeholderHeight}; real geometry from in-stream SPS)")
    }

    /**
     * Push one Annex-B access unit into the decoder. Blocks up to
     * [timeoutUs] for an input buffer; if none, drops the frame.
     * Returns true if the frame was queued.
     */
    fun feed(annexB: ByteArray, presentationTimeUs: Long, isKeyframe: Boolean, timeoutUs: Long = 10_000L): Boolean {
        val codec = this.codec ?: return false
        val idx = codec.dequeueInputBuffer(timeoutUs)
        if (idx < 0) return false
        val buf = codec.getInputBuffer(idx) ?: return false
        buf.clear()
        if (annexB.size > buf.capacity()) {
            Log.w(TAG, "frame ${annexB.size} > input buffer ${buf.capacity()}, dropping")
            // Re-queue the buffer empty so it's not lost.
            codec.queueInputBuffer(idx, 0, 0, presentationTimeUs, 0)
            return false
        }
        buf.put(annexB)
        val flags = if (isKeyframe) MediaCodec.BUFFER_FLAG_KEY_FRAME else 0
        codec.queueInputBuffer(idx, 0, annexB.size, presentationTimeUs, flags)
        return true
    }

    /**
     * Drain any decoded output buffers and release them to the surface.
     * Call repeatedly (e.g. from the same coroutine as [feed]) so the
     * decoder doesn't back up.
     *
     * Returns true if at least one buffer was rendered.
     */
    fun drain(timeoutUs: Long = 0L): Boolean {
        val codec = this.codec ?: return false
        var rendered = false
        while (true) {
            val idx = codec.dequeueOutputBuffer(bufferInfo, timeoutUs)
            when {
                idx >= 0 -> {
                    val isEos = (bufferInfo.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM) != 0
                    // render = true → send to the configured surface
                    codec.releaseOutputBuffer(idx, /* render = */ true)
                    rendered = true
                    if (isEos) return rendered
                }
                idx == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                    val newFormat = codec.outputFormat
                    Log.i(TAG, "output format: ${newFormat.getInteger(MediaFormat.KEY_WIDTH)}x${newFormat.getInteger(MediaFormat.KEY_HEIGHT)}")
                }
                idx == MediaCodec.INFO_TRY_AGAIN_LATER -> return rendered
                idx == MediaCodec.INFO_OUTPUT_BUFFERS_CHANGED -> {
                    // Deprecated since API 21 — handled internally by
                    // MediaCodec. We just continue.
                }
                else -> return rendered
            }
        }
    }

    fun stop() {
        if (!started) return
        started = false
        try {
            codec?.stop()
        } catch (e: Exception) {
            Log.w(TAG, "stop", e)
        }
        try {
            codec?.release()
        } catch (e: Exception) {
            Log.w(TAG, "release", e)
        }
        codec = null
    }

    companion object {
        private const val TAG = "MoonBeam.Decoder"
    }
}
