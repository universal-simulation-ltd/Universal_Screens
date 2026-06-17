package com.universalsim.extender

import android.content.Context
import android.net.ConnectivityManager
import android.net.Network
import android.net.NetworkCapabilities
import android.net.NetworkRequest
import android.net.wifi.WifiNetworkSpecifier
import android.os.Build
import android.os.Handler
import android.os.Looper
import androidx.annotation.RequiresApi

/**
 * Join a specific Wi-Fi network on demand (Android 10+), so one scanned QR can put
 * the phone on the host's network and then connect.
 *
 * Uses [WifiNetworkSpecifier]: the OS shows a one-tap "Connect to <SSID>?" dialog —
 * app-initiated joins can't be silent, by design. On success we bind the process
 * to that network so the clicker's socket routes over it (the network has no
 * internet, which is fine for a LAN host). The request is kept alive for the
 * session; call [release] to drop it.
 */
object WifiConnect {
    private var callback: ConnectivityManager.NetworkCallback? = null

    /** WifiNetworkSpecifier joins need Android 10 (API 29). */
    fun isSupported(): Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q

    @RequiresApi(Build.VERSION_CODES.Q)
    fun join(
        context: Context,
        ssid: String,
        password: String?,
        auth: String,
        onResult: (Boolean) -> Unit,
    ) {
        val cm = context.applicationContext.getSystemService(ConnectivityManager::class.java)
        release(cm)

        val spec = WifiNetworkSpecifier.Builder().setSsid(ssid).apply {
            if (!password.isNullOrEmpty() && auth != "nopass") setWpa2Passphrase(password)
        }.build()
        val request = NetworkRequest.Builder()
            .addTransportType(NetworkCapabilities.TRANSPORT_WIFI)
            // The host's Wi-Fi often has no internet; don't require it.
            .removeCapability(NetworkCapabilities.NET_CAPABILITY_INTERNET)
            .setNetworkSpecifier(spec)
            .build()

        val main = Handler(Looper.getMainLooper())
        var fired = false
        val cb = object : ConnectivityManager.NetworkCallback() {
            override fun onAvailable(network: Network) {
                cm.bindProcessToNetwork(network) // route our sockets over this Wi-Fi
                if (!fired) { fired = true; main.post { onResult(true) } }
            }
            override fun onUnavailable() {
                if (!fired) { fired = true; main.post { onResult(false) } }
            }
        }
        callback = cb
        // 30s: enough for the user to accept the system dialog; onUnavailable fires otherwise.
        cm.requestNetwork(request, cb, 30_000)
    }

    /** Drop any app-requested Wi-Fi and unbind the process from it. */
    fun release(cm: ConnectivityManager) {
        callback?.let { runCatching { cm.unregisterNetworkCallback(it) } }
        callback = null
        runCatching { cm.bindProcessToNetwork(null) }
    }
}
