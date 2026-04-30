package com.m151.moonbeam

import android.os.Bundle
import android.os.SystemClock
import android.util.Log
import android.view.SurfaceHolder
import android.view.SurfaceView
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import com.m151.moonbeam.decode.VideoDecoder
import com.m151.moonbeam.input.TouchHandler
import com.m151.moonbeam.net.WsClient
import com.m151.moonbeam.protocol.InputMsg
import com.m151.moonbeam.protocol.Wire
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { MoonBeamRoot() }
    }
}

class MoonBeamViewModel : ViewModel() {
    private val ws = WsClient()
    private val _status = MutableStateFlow(Status(state = ConnState.IDLE, lastError = null, framesDecoded = 0))
    val status: StateFlow<Status> = _status.asStateFlow()
    private val _stats = MutableStateFlow(Stats())
    val stats: StateFlow<Stats> = _stats.asStateFlow()

    @Volatile private var decoder: VideoDecoder? = null

    // FPS bookkeeping — count frames decoded in the last second.
    private var fpsWindowStartMs = SystemClock.elapsedRealtime()
    private var fpsWindowFrames = 0

    fun attachDecoder(dec: VideoDecoder) {
        decoder = dec
        if (status.value.state == ConnState.IDLE) connect()
    }

    fun detachDecoder() {
        decoder = null
    }

    /**
     * Forward an input event. [eventTimeMs] is `MotionEvent.getEventTime()`
     * — the SystemClock.uptimeMillis() at which the kernel observed
     * the input. Difference from now-on-the-wire is our pre-wire input
     * latency.
     */
    fun sendInput(msg: InputMsg, eventTimeMs: Long): Boolean {
        val ok = ws.send(msg)
        if (ok) {
            val ageMs = SystemClock.uptimeMillis() - eventTimeMs
            _stats.value = _stats.value.copy(inputLatencyMs = ageMs)
        }
        return ok
    }

    private fun connect() {
        // Periodic ping for round-trip latency. 1Hz is plenty.
        viewModelScope.launch {
            while (true) {
                delay(1_000)
                if (status.value.state == ConnState.CONNECTED) {
                    val ts = System.nanoTime() / 1_000
                    ws.sendPing(ts)
                }
            }
        }

        viewModelScope.launch {
            // Retry forever — the host may come and go; the app
            // shouldn't need to be relaunched. 1s backoff is fine for
            // M3, no need for jittered exponential at this scale.
            while (true) {
                _status.value = _status.value.copy(state = ConnState.CONNECTING)
                ws.connect().collect { ev ->
                    when (ev) {
                        is WsClient.Event.Open -> {
                            _status.value = _status.value.copy(
                                state = ConnState.CONNECTED,
                                lastError = null,
                            )
                        }
                        is WsClient.Event.Frame -> handleFrame(ev.inbound)
                        is WsClient.Event.Closed -> {
                            _status.value = _status.value.copy(
                                state = ConnState.IDLE,
                                lastError = "closed: ${ev.code} ${ev.reason}",
                            )
                        }
                        is WsClient.Event.Failure -> {
                            _status.value = _status.value.copy(
                                state = ConnState.IDLE,
                                lastError = ev.cause.message ?: ev.cause::class.java.simpleName,
                            )
                        }
                    }
                }
                delay(1_000)
            }
        }
    }

    private fun handleFrame(inbound: Wire.Inbound) {
        when (inbound) {
            is Wire.Inbound.Video -> {
                val dec = decoder ?: return
                val arrivalUs = System.nanoTime() / 1_000
                val pts = arrivalUs
                if (dec.feed(inbound.annexB, pts, inbound.isKeyframe)) {
                    if (dec.drain()) {
                        val drainedUs = System.nanoTime() / 1_000
                        val decodeUs = drainedUs - arrivalUs
                        // Update FPS once per second.
                        fpsWindowFrames++
                        val nowMs = SystemClock.elapsedRealtime()
                        val windowMs = nowMs - fpsWindowStartMs
                        val newFps = if (windowMs >= 1000) {
                            val fps = (fpsWindowFrames * 1000.0 / windowMs).toInt()
                            fpsWindowStartMs = nowMs
                            fpsWindowFrames = 0
                            fps
                        } else {
                            _stats.value.fps
                        }
                        _stats.value = _stats.value.copy(
                            decodeLatencyUs = decodeUs,
                            fps = newFps,
                        )
                        _status.value = _status.value.copy(
                            framesDecoded = _status.value.framesDecoded + 1,
                        )
                    }
                }
            }
            is Wire.Inbound.Pong -> {
                val nowUs = System.nanoTime() / 1_000
                val rttUs = nowUs - inbound.timestampUs
                _stats.value = _stats.value.copy(wsRttUs = rttUs)
            }
            is Wire.Inbound.Input -> Unit // host doesn't send input, ignore
        }
    }

    override fun onCleared() {
        decoder?.stop()
        super.onCleared()
    }

    data class Status(val state: ConnState, val lastError: String?, val framesDecoded: Int)
    enum class ConnState { IDLE, CONNECTING, CONNECTED }

    /**
     * Latency / throughput numbers visible in the debug overlay.
     * Decoder + FPS measured per-frame; input pre-wire age measured
     * per `MotionEvent`; RTT measured per ping/pong (1 Hz).
     */
    data class Stats(
        val decodeLatencyUs: Long = 0,
        val fps: Int = 0,
        val inputLatencyMs: Long = 0,
        val wsRttUs: Long = 0,
    )
}

@Composable
fun MoonBeamRoot(viewModel: MoonBeamViewModel = viewModel()) {
    val status by viewModel.status.collectAsState()
    val stats by viewModel.stats.collectAsState()

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black),
    ) {
        AndroidView(
            modifier = Modifier.fillMaxSize(),
            factory = { ctx ->
                SurfaceView(ctx).apply {
                    isFocusable = true
                    isFocusableInTouchMode = true
                    val touchHandler = TouchHandler(send = { msg, eventTimeMs ->
                        viewModel.sendInput(msg, eventTimeMs)
                    })
                    setOnTouchListener { _, event -> touchHandler.handle(event) }
                    setOnHoverListener { _, event -> touchHandler.handle(event) }
                    holder.addCallback(object : SurfaceHolder.Callback {
                        private var localDecoder: VideoDecoder? = null
                        override fun surfaceCreated(holder: SurfaceHolder) {
                            val dec = VideoDecoder(holder.surface)
                            try {
                                dec.start()
                                localDecoder = dec
                                viewModel.attachDecoder(dec)
                            } catch (e: Exception) {
                                Log.e("MoonBeam.Surface", "decoder start failed", e)
                            }
                        }
                        override fun surfaceChanged(holder: SurfaceHolder, format: Int, width: Int, height: Int) {
                            // Aspect-fit happens at the SurfaceView level —
                            // KWin tells us geometry via future control
                            // messages; for now Android scales the surface.
                        }
                        override fun surfaceDestroyed(holder: SurfaceHolder) {
                            viewModel.detachDecoder()
                            localDecoder?.stop()
                            localDecoder = null
                        }
                    })
                }
            },
        )

        if (status.framesDecoded == 0) {
            StatusOverlay(status, modifier = Modifier.align(Alignment.Center))
        } else {
            // Once video is flowing, show the latency stats in the
            // top-right. Always-on, low-attention.
            StatsOverlay(stats, modifier = Modifier.align(Alignment.TopEnd))
        }
    }
}

@Composable
private fun StatusOverlay(status: MoonBeamViewModel.Status, modifier: Modifier = Modifier) {
    val text = when (status.state) {
        MoonBeamViewModel.ConnState.IDLE -> buildString {
            append("MoonBeam — waiting for connection")
            status.lastError?.let { append("\n").append(it) }
        }
        MoonBeamViewModel.ConnState.CONNECTING -> "MoonBeam — connecting to ${WsClient.DEFAULT_URL}"
        MoonBeamViewModel.ConnState.CONNECTED -> "MoonBeam — connected, waiting for first keyframe"
    }
    Text(
        text = text,
        color = Color(0xFF22CC55),
        modifier = modifier.padding(24.dp),
    )
}

@Composable
private fun StatsOverlay(stats: MoonBeamViewModel.Stats, modifier: Modifier = Modifier) {
    val text = buildString {
        append("fps:    ").append(stats.fps).append('\n')
        append("decode: ").append(stats.decodeLatencyUs / 1000).append(" ms\n")
        append("input:  ").append(stats.inputLatencyMs).append(" ms\n")
        append("ws rtt: ").append(stats.wsRttUs / 1000).append(" ms")
    }
    Text(
        text = text,
        color = Color(0x9922CC55),
        fontSize = 11.sp,
        fontFamily = FontFamily.Monospace,
        modifier = modifier.padding(8.dp),
    )
}
