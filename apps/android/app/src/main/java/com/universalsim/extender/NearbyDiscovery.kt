package com.universalsim.extender

import android.content.Context
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.os.Handler
import android.os.Looper

/** A serving desktop host discovered over DNS-SD. [addr] is `ip:port`, ready
 *  for the connect flow. */
data class NearbyHost(val serviceName: String, val name: String, val addr: String)

/**
 * Browses DNS-SD for serving Universal Screens hosts (`_usscreens._tcp`,
 * advertised by the desktop hosts while they serve — see the shared
 * `extender-discovery` crate) and keeps a resolved list, so the connect screen
 * can offer nearby hosts with no QR scan.
 *
 * NSD quirks handled here:
 *  - `resolveService` must not run concurrently (the framework returns
 *    FAILURE_ALREADY_ACTIVE), so found services queue and resolve one at a time.
 *  - Callbacks arrive on a binder thread; [onChange] is posted to the UI thread.
 *
 * `resolveService` is deprecated as of API 34 (in favour of
 * `registerServiceInfoCallback`) but present and functional on every level we
 * ship (minSdk 24); switch when minSdk reaches 34.
 */
class NearbyDiscovery(context: Context, private val onChange: (List<NearbyHost>) -> Unit) {

    companion object {
        /** Matches `MDNS_SERVICE_TYPE` in `crates/discovery` (sans `.local.`). */
        const val SERVICE_TYPE = "_usscreens._tcp."
    }

    private val nsd = context.applicationContext.getSystemService(Context.NSD_SERVICE) as NsdManager
    private val ui = Handler(Looper.getMainLooper())

    // All state is confined to the UI thread (callbacks post before touching it).
    private val hosts = LinkedHashMap<String, NearbyHost>() // keyed by service name
    private val resolveQueue = ArrayDeque<NsdServiceInfo>()
    private var resolving = false
    private var listener: NsdManager.DiscoveryListener? = null

    fun start() {
        if (listener != null) return
        val l = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(serviceType: String) {}
            override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
                ui.post { listener = null }
            }
            override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {}
            override fun onDiscoveryStopped(serviceType: String) {}
            override fun onServiceFound(info: NsdServiceInfo) {
                ui.post {
                    resolveQueue.addLast(info)
                    resolveNext()
                }
            }
            override fun onServiceLost(info: NsdServiceInfo) {
                ui.post {
                    if (hosts.remove(info.serviceName) != null) publish()
                }
            }
        }
        listener = l
        runCatching { nsd.discoverServices(SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, l) }
            .onFailure { listener = null }
    }

    fun stop() {
        listener?.let { l -> runCatching { nsd.stopServiceDiscovery(l) } }
        listener = null
        hosts.clear()
        resolveQueue.clear()
        resolving = false
        publish()
    }

    private fun resolveNext() {
        if (resolving) return
        val next = resolveQueue.removeFirstOrNull() ?: return
        resolving = true
        @Suppress("DEPRECATION")
        nsd.resolveService(next, object : NsdManager.ResolveListener {
            override fun onResolveFailed(info: NsdServiceInfo, errorCode: Int) {
                ui.post {
                    resolving = false
                    resolveNext()
                }
            }
            override fun onServiceResolved(info: NsdServiceInfo) {
                ui.post {
                    resolving = false
                    val ip = info.host?.hostAddress
                    if (ip != null && info.port > 0) {
                        hosts[info.serviceName] = NearbyHost(
                            serviceName = info.serviceName,
                            name = info.serviceName,
                            addr = "$ip:${info.port}",
                        )
                        publish()
                    }
                    resolveNext()
                }
            }
        })
    }

    private fun publish() = onChange(hosts.values.toList())
}
