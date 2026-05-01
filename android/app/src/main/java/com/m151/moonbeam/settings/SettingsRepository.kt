package com.m151.moonbeam.settings

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

private val Context.dataStore: DataStore<Preferences> by preferencesDataStore(name = "settings")

class SettingsRepository(private val context: Context) {
    private val HOST_URL = stringPreferencesKey("host_url")
    private val SHOW_STATS = booleanPreferencesKey("show_stats")
    private val SHOW_WIRE_DEBUG = booleanPreferencesKey("show_wire_debug")
    private val VERBOSE_LOGGING = booleanPreferencesKey("verbose_logging")
    private val MODE = stringPreferencesKey("mode")
    private val QUALITY = stringPreferencesKey("quality")
    private val AUDIO_ENABLED = booleanPreferencesKey("audio_enabled")
    private val PRESSURE_CURVE = stringPreferencesKey("pressure_curve")
    private val STYLUS_BUTTON = stringPreferencesKey("stylus_button")

    val settingsFlow: Flow<AppSettings> = context.dataStore.data.map { preferences ->
        AppSettings(
            hostUrl = preferences[HOST_URL] ?: "ws://127.0.0.1:7878/ws",
            showStats = preferences[SHOW_STATS] ?: false,
            showWireDebug = preferences[SHOW_WIRE_DEBUG] ?: false,
            verboseLogging = preferences[VERBOSE_LOGGING] ?: false,
            mode = ConnectionMode.valueOf(preferences[MODE] ?: ConnectionMode.Extended.name),
            quality = QualityMode.valueOf(preferences[QUALITY] ?: QualityMode.Display.name),
            audioEnabled = preferences[AUDIO_ENABLED] ?: false,
            pressureCurve = PressureCurve.valueOf(preferences[PRESSURE_CURVE] ?: PressureCurve.Linear.name),
            stylusButton = StylusBinding.valueOf(preferences[STYLUS_BUTTON] ?: StylusBinding.RightClick.name)
        )
    }

    suspend fun updateSettings(transform: (AppSettings) -> AppSettings) {
        // Implementation for phase 4, but provided as a stub
    }
}
