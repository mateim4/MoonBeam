package com.m151.moonbeam

import android.os.Bundle
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
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import com.m151.moonbeam.decode.VideoDecoder
import com.m151.moonbeam.net.WsClient
import com.m151.moonbeam.protocol.Wire
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * M3 step 2 — fullscreen video receiver.
 *
 * Single Activity, single Composable that hosts a [SurfaceView] (via
 * AndroidView). The surface's lifecycle drives the decoder's
 * lifecycle: when the surface is ready, we hand it to a fresh
 * [VideoDecoder]; when it goes away, we tear the decoder down.
 *
 * The WebSocket connection runs in the [MoonBeamViewModel] scope so
 * it survives configuration changes (and locks them off via
 * `configChanges` on the Activity in the manifest).
 */
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

    @Volatile private var decoder: VideoDecoder? = null

    fun attachDecoder(dec: VideoDecoder) {
        decoder = dec
        if (status.value.state == ConnState.IDLE) connect()
    }

    fun detachDecoder() {
        decoder = null
    }

    private fun connect() {
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
                        is WsClient.Event.Frame -> {
                            val inbound = ev.inbound
                            if (inbound is Wire.Inbound.Video) {
                                val dec = decoder
                                if (dec != null) {
                                    val nowUs = System.nanoTime() / 1_000
                                    if (dec.feed(inbound.annexB, nowUs, inbound.isKeyframe)) {
                                        if (dec.drain()) {
                                            _status.value = _status.value.copy(
                                                framesDecoded = _status.value.framesDecoded + 1,
                                            )
                                        }
                                    }
                                }
                            }
                        }
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

    override fun onCleared() {
        decoder?.stop()
        super.onCleared()
    }

    data class Status(val state: ConnState, val lastError: String?, val framesDecoded: Int)
    enum class ConnState { IDLE, CONNECTING, CONNECTED }
}

@Composable
fun MoonBeamRoot(viewModel: MoonBeamViewModel = viewModel()) {
    val status by viewModel.status.collectAsState()

    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black),
    ) {
        AndroidView(
            modifier = Modifier.fillMaxSize(),
            factory = { ctx ->
                SurfaceView(ctx).apply {
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
