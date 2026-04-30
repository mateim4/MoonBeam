package com.m151.moonbeam.net

import android.util.Log
import com.m151.moonbeam.protocol.InputMsg
import com.m151.moonbeam.protocol.Wire
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import java.util.concurrent.TimeUnit

/**
 * Thin OkHttp WebSocket wrapper that speaks our locked frame format.
 *
 * Outbound: input events via [send], encoded as `[0x03][flags][json]`.
 * Inbound: video access units (and future audio) emitted as a Flow.
 *
 * This is the M3-minimum implementation: one connection, no
 * reconnect-with-backoff, no auth, no TLS. The host is hardcoded to
 * `ws://127.0.0.1:7878/ws` (i.e. `adb reverse tcp:7878 tcp:7878`).
 * Reconnection / pairing / mDNS land in M4.
 */
class WsClient(
    private val url: String = DEFAULT_URL,
) {
    private val httpClient: OkHttpClient by lazy {
        OkHttpClient.Builder()
            // Server pings us out of band; if we miss too many we
            // assume the connection is wedged.
            .pingInterval(5, TimeUnit.SECONDS)
            // Don't ever close idle connections — once we have a
            // session we keep it for the life of the activity.
            .readTimeout(0, TimeUnit.MILLISECONDS)
            .build()
    }
    private var socket: WebSocket? = null

    /**
     * Connect and emit inbound frames. Cold flow — collect to start.
     * Closing the flow closes the WS.
     */
    fun connect(): Flow<Event> = callbackFlow {
        val request = Request.Builder().url(url).build()
        val listener = object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                Log.i(TAG, "ws open: $url")
                trySend(Event.Open)
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                // Server only ever sends binary frames in our protocol;
                // text frames are reserved.
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                val frame = bytes.toByteArray()
                val inbound = Wire.decodeInbound(frame) ?: return
                trySend(Event.Frame(inbound))
            }

            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                Log.i(TAG, "ws closing: $code $reason")
                webSocket.close(code, reason)
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                Log.i(TAG, "ws closed: $code $reason")
                trySend(Event.Closed(code, reason))
                close()
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                Log.w(TAG, "ws failure", t)
                trySend(Event.Failure(t))
                // Close the flow normally — emit the failure as an
                // Event, don't propagate the throwable as a flow
                // cancellation cause. close(t) re-throws on the
                // collector side and crashes the Activity.
                close()
            }
        }
        socket = httpClient.newWebSocket(request, listener)

        awaitClose {
            socket?.close(1000, "client closing")
            socket = null
        }
    }

    /**
     * Send an [InputMsg] over the WS. Returns true if the OkHttp
     * outbound queue accepted the message (which means it's enqueued,
     * not necessarily on the wire yet — OkHttp handles backpressure).
     */
    fun send(msg: InputMsg): Boolean {
        val socket = this.socket ?: return false
        val frame = Wire.encodeInput(msg)
        return socket.send(ByteString.of(*frame))
    }

    /**
     * Send a ping with the given monotonic timestamp (microseconds).
     * Server echoes it back as PONG with the same payload.
     */
    fun sendPing(timestampUs: Long): Boolean {
        val socket = this.socket ?: return false
        val frame = Wire.encodePing(timestampUs)
        return socket.send(ByteString.of(*frame))
    }

    sealed class Event {
        data object Open : Event()
        data class Frame(val inbound: Wire.Inbound) : Event()
        data class Closed(val code: Int, val reason: String) : Event()
        data class Failure(val cause: Throwable) : Event()
    }

    companion object {
        private const val TAG = "MoonBeam.Ws"
        // adb reverse tcp:7878 tcp:7878 maps the host's :7878 to the
        // tablet's localhost:7878. Same wire format on USB-C and LAN
        // (M4); for now we only speak USB.
        const val DEFAULT_URL = "ws://127.0.0.1:7878/ws"
    }
}
