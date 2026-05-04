package com.m151.moonbeam.settings

data class AppSettings(
    val hostUrl: String = "ws://127.0.0.1:7878/ws",
    val showStats: Boolean = false,
    val showWireDebug: Boolean = false,
    val verboseLogging: Boolean = false,
    val mode: ConnectionMode = ConnectionMode.Extended,
    val quality: QualityMode = QualityMode.Display,
    val audioEnabled: Boolean = false,
    val pressureCurve: PressureCurve = PressureCurve.Linear,
    val stylusButton: StylusBinding = StylusBinding.RightClick,
)

enum class ConnectionMode { Extended, Mirror }
enum class QualityMode { Drawing, Display }
enum class PressureCurve { Linear, Soft, Hard }
enum class StylusBinding { RightClick, MiddleClick, Undo, Disabled }
