package com.m151.moonbeam

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp

/**
 * M3 step 1 — project skeleton.
 *
 * Empty Compose shell that proves the build wires up. M3 step 2 lands
 * here — replace the placeholder Box with an `AndroidView { SurfaceView(it) }`
 * and wire in the MediaCodec decoder.
 */
class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { MoonBeamRoot() }
    }
}

@Composable
fun MoonBeamRoot() {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            text = "MoonBeam — M3 step 1\nproject skeleton, no decoder yet",
            color = Color(0xFF22CC55),
            textAlign = TextAlign.Center,
            modifier = Modifier.padding(24.dp),
        )
    }
}
