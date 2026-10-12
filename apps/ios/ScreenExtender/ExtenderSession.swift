import Foundation

/// Swift wrapper over the C FFI in `extender_ffi.h` (the `extender-mobile-ffi`
/// crate). Owns the opaque session pointer. Connect off the main thread — it does
/// blocking network I/O.
///
/// For the clicker, `startPump` drains downstream events (slide previews, host
/// identity, window list) on a background thread and delivers them on the main
/// queue. The Start/Frame video events are ignored here (a future viewer mode
/// would feed them to VideoToolbox).
final class ExtenderSession {
    /// Mirrors `EXTENDER_CAPTURE_*` in `extender_ffi.h`.
    enum CaptureMode: UInt32 {
        case virtualDisplay = 0
        case mirror = 1
        case controlOnly = 2
    }

    private var handle: OpaquePointer?
    private var pumpThread: Thread?
    private var pumping = false

    private init(handle: OpaquePointer) { self.handle = handle }

    /// A connect's result: `session` when it worked, else `failure` says why.
    struct Outcome {
        let session: ExtenderSession?
        let failure: ConnectFailure?
    }

    /// Blocking connect: the session, or why there is none. Call off the main
    /// thread. `deviceName` is sent to the host to label the virtual display
    /// (e.g. "James's iPhone"); pass "" for none.
    static func connect(
        addr: String,
        width: UInt32 = 1920,
        height: UInt32 = 1080,
        mode: CaptureMode,
        pin: UInt32 = 0,
        deviceName: String = ""
    ) -> Outcome {
        let handle = addr.withCString { addrPtr in
            deviceName.withCString { namePtr in
                extender_session_connect(addrPtr, width, height, mode.rawValue, pin, namePtr)
            }
        }
        if let handle { return Outcome(session: ExtenderSession(handle: handle), failure: nil) }
        // ⚠️ Read straight away, on this same thread: the core keeps the reason
        // per thread, so an overlapping attempt can't swap it.
        let problem = ConnectProblem(rawValue: extender_last_connect_problem()) ?? .other
        return Outcome(session: nil,
                       failure: ConnectFailure(problem: problem, waitSeconds: extender_last_connect_wait_seconds()))
    }

    // MARK: - Downstream events

    /// Sink for downstream events. The clicker callbacks (`onSnapshot` /
    /// `onHostInfo` / `onWindowList` / `onEnded`) are delivered on the main queue;
    /// the video callbacks (`onStart` / `onFrame`) are delivered on the pump thread
    /// so decoding stays off the main thread (they don't touch SwiftUI state).
    struct Sink {
        /// Stream start: `codec` is 0 = H.264, 1 = HEVC; `csd` = Annex-B param sets.
        var onStart: (_ width: Int, _ height: Int, _ codec: Int, _ csd: Data) -> Void = { _, _, _, _ in }
        /// One encoded frame: `data` = Annex-B NAL units.
        var onFrame: (_ data: Data, _ keyframe: Bool, _ ptsValue: Int64) -> Void = { _, _, _ in }
        /// A slide preview: `slot` is 0 = current, -1 = previous, +1 = next; `jpeg`
        /// is empty when there's no slide there.
        var onSnapshot: (_ slot: Int32, _ jpeg: Data) -> Void = { _, _ in }
        var onHostInfo: (_ os: String, _ name: String) -> Void = { _, _ in }
        var onWindowList: (_ windows: [(id: Int64, title: String)]) -> Void = { _ in }
        var onEnded: () -> Void = {}
    }

    /// Drain events on a background thread until the stream ends. Idempotent.
    func startPump(_ sink: Sink) {
        guard let handle, !pumping else { return }
        pumping = true
        let thread = Thread { [weak self] in
            while self?.pumping == true {
                guard let event = extender_session_next_event(handle) else { break }
                let kind = extender_event_kind(event)
                let data = Self.eventData(event)
                switch kind {
                case EXTENDER_EVENT_START:
                    let w = Int(extender_event_width(event))
                    let h = Int(extender_event_height(event))
                    let codec = Int(extender_event_codec(event))
                    sink.onStart(w, h, codec, data) // pump thread (decoder is off-main)
                case EXTENDER_EVENT_FRAME:
                    sink.onFrame(data, extender_event_keyframe(event), extender_event_pts_value(event))
                case EXTENDER_EVENT_SNAPSHOT:
                    let slot = extender_event_slot(event)
                    DispatchQueue.main.async { sink.onSnapshot(slot, data) }
                case EXTENDER_EVENT_HOSTINFO:
                    let (os, name) = Self.split2(data, separator: "\n")
                    DispatchQueue.main.async { sink.onHostInfo(os, name) }
                case EXTENDER_EVENT_WINDOWLIST:
                    let windows = Self.parseWindows(data)
                    DispatchQueue.main.async { sink.onWindowList(windows) }
                default:
                    break // unknown event kind
                }
                extender_event_free(event)
            }
            DispatchQueue.main.async { sink.onEnded() }
        }
        thread.name = "extender-pump"
        pumpThread = thread
        thread.start()
    }

    // MARK: - Input

    /// A key tap: down then up. The clicker's workhorse.
    func tapKey(_ hid: UInt32) {
        guard let handle else { return }
        extender_send_key(handle, hid, true)
        extender_send_key(handle, hid, false)
    }

    func sendText(_ text: String) {
        guard let handle else { return }
        text.withCString { extender_send_text(handle, $0) }
    }

    func sendTouch(id: UInt32, phase: ExtenderTouchPhase, x: Float, y: Float) {
        guard let handle else { return }
        extender_send_touch(handle, id, phase, x, y)
    }

    func sendMouseMoveRelative(dx: Float, dy: Float) {
        guard let handle else { return }
        extender_send_mouse_move_relative(handle, dx, dy)
    }

    func sendMouseButton(button: Int32, pressed: Bool) {
        guard let handle else { return }
        let btn: ExtenderMouseButton = switch button {
        case 1: EXTENDER_MOUSE_RIGHT
        case 2: EXTENDER_MOUSE_MIDDLE
        default: EXTENDER_MOUSE_LEFT
        }
        extender_send_mouse_button(handle, btn, pressed)
    }

    func sendScroll(dx: Float, dy: Float) {
        guard let handle else { return }
        extender_send_scroll(handle, dx, dy)
    }

    /// Pre-scan the open document so the host can preview adjacent slides.
    func scanDeck() {
        guard let handle else { return }
        extender_send_scan_deck(handle)
    }

    /// (Re)request the host's window list (a window-list event follows).
    func listWindows() {
        guard let handle else { return }
        extender_send_list_windows(handle)
    }

    /// Bring host window `id` to the foreground; `startShow` also starts its slideshow.
    func focusWindow(id: Int64, startShow: Bool) {
        guard let handle else { return }
        extender_send_focus_window(handle, id, startShow)
    }

    // MARK: - Lifecycle

    func close() {
        pumping = false
        guard let handle else { return }
        // NOTE: the pump may still be blocked in extender_session_next_event; the
        // session free shuts the socket so it returns and the thread exits. (Same
        // lifecycle caveat as the Android pump.)
        extender_session_free(handle)
        self.handle = nil
    }

    deinit { close() }

    // MARK: - Helpers

    private static func eventData(_ event: OpaquePointer) -> Data {
        var len = 0
        guard let ptr = extender_event_data(event, &len), len > 0 else { return Data() }
        return Data(bytes: ptr, count: len)
    }

    private static func split2(_ data: Data, separator: Character) -> (String, String) {
        let text = String(decoding: data, as: UTF8.self)
        let parts = text.split(separator: separator, maxSplits: 1, omittingEmptySubsequences: false)
        return (parts.first.map(String.init) ?? "", parts.count > 1 ? String(parts[1]) : "")
    }

    private static func parseWindows(_ data: Data) -> [(id: Int64, title: String)] {
        let text = String(decoding: data, as: UTF8.self)
        guard !text.isEmpty else { return [] }
        return text.split(separator: "\n").compactMap { line in
            guard let tab = line.firstIndex(of: "\t"), let id = Int64(line[..<tab]) else { return nil }
            return (id, String(line[line.index(after: tab)...]))
        }
    }
}

/// Why a connect failed — the Rust core's `ConnectProblem`, read through
/// `extender_last_connect_problem` (`EXTENDER_PROBLEM_*` in `extender_ffi.h`).
/// The numbers are the core's and never change meaning.
enum ConnectProblem: Int32 {
    case other = 1, badAddress, unreachable, noAnswer, dropped, wrongCode,
         hostLocked, needsCode, unknownHost, olderHost, unsupported
}

/// A failed connect: why, and (for `.hostLocked`) how long to wait.
struct ConnectFailure: Equatable {
    let problem: ConnectProblem
    var waitSeconds: UInt32 = 0
}

/// The sentences shown for a failed connect, in the knowledge base's eight
/// languages (the reader's saved choice, else the device's — see
/// `Knowledge.language`). Mirrors `Messages.kt` on Android word for word.
///
/// ⚠️ No Rust error text ever reaches the screen: it is English, technical,
/// and says nothing about what to do.
enum ConnectMessages {
    static func title(_ lang: String) -> String {
        switch lang {
        case "fr": "Échec de la connexion"
        case "es": "No se ha podido conectar"
        case "it": "Connessione non riuscita"
        case "de": "Verbindung fehlgeschlagen"
        case "pt-BR": "Falha na conexão"
        case "pt-PT": "Falha na ligação"
        case "tr": "Bağlantı kurulamadı"
        default: "Connection failed"
        }
    }

    static func body(_ lang: String, _ failure: ConnectFailure?) -> String {
        let f = failure ?? ConnectFailure(problem: .other)
        let line = table(lang)[Int(f.problem.rawValue) - 1]
        return line.replacingOccurrences(of: "%d", with: String(max(f.waitSeconds, 1)))
    }

    /// One sentence per `ConnectProblem`, in code order (`.other` first).
    private static func table(_ lang: String) -> [String] {
        switch lang {
        case "fr": [
            "Vérifiez que Universal Screens est ouvert sur l'ordinateur, que le code PIN correspond et que les deux appareils sont sur le même Wi-Fi.",
            "Cette adresse est introuvable. Vérifiez-la, ou scannez plutôt le QR code de l'ordinateur.",
            "Rien n'a répondu à cette adresse. Vérifiez que Universal Screens est ouvert sur l'ordinateur, que l'ordinateur n'est pas en veille et que les deux appareils sont sur le même Wi-Fi.",
            "Quelque chose a répondu à cette adresse, mais ce n'est pas Universal Screens, ou un pare-feu bloque la connexion. Vérifiez l'adresse et le port, ou scannez le QR code de l'ordinateur.",
            "L'ordinateur a fermé la connexion pendant sa mise en place. Réessayez.",
            "Ce code PIN ne correspond pas à celui affiché sur l'ordinateur. Vérifiez-le et réessayez.",
            "Trop de codes PIN erronés. L'ordinateur fait une pause ; réessayez dans %d s.",
            "Cet ordinateur demande son code PIN. Scannez son QR code, ou saisissez le code PIN qu'il affiche.",
            "Un autre ordinateur a répondu à cette adresse. Scannez le QR code de l'ordinateur voulu, ou saisissez son code PIN.",
            "L'ordinateur utilise une ancienne version de Universal Screens. Mettez-la à jour, puis réessayez.",
            "L'ordinateur utilise une autre version de Universal Screens. Mettez les deux applications à jour, puis réessayez.",
        ]
        case "es": [
            "Compruebe que Universal Screens está abierto en el ordenador, que el PIN coincide y que ambos dispositivos están en la misma red Wi-Fi.",
            "No se encuentra esa dirección. Compruébela, o escanee el código QR del ordenador.",
            "No ha respondido nada en esa dirección. Compruebe que Universal Screens está abierto en el ordenador, que el ordenador no está en reposo y que ambos dispositivos están en la misma red Wi-Fi.",
            "Algo ha respondido en esa dirección, pero no es Universal Screens, o un cortafuegos está bloqueando la conexión. Compruebe la dirección y el puerto, o escanee el código QR del ordenador.",
            "El ordenador ha cerrado la conexión mientras se establecía. Vuelva a intentarlo.",
            "Ese PIN no coincide con el que muestra el ordenador. Compruébelo y vuelva a intentarlo.",
            "Demasiados PIN incorrectos. El ordenador está en pausa; vuelva a intentarlo dentro de %d s.",
            "Este ordenador necesita su PIN. Escanee su código QR o introduzca el PIN que muestra.",
            "Ha respondido otro ordenador en esa dirección. Escanee el código QR del ordenador que quiere usar o introduzca su PIN.",
            "El ordenador tiene una versión antigua de Universal Screens. Actualícela y vuelva a intentarlo.",
            "El ordenador tiene otra versión de Universal Screens. Actualice las dos aplicaciones y vuelva a intentarlo.",
        ]
        case "it": [
            "Controlla che Universal Screens sia aperto sul computer, che il PIN corrisponda e che entrambi i dispositivi siano sulla stessa rete Wi-Fi.",
            "Impossibile trovare questo indirizzo. Controllalo, oppure scansiona il codice QR del computer.",
            "Nessuna risposta a questo indirizzo. Controlla che Universal Screens sia aperto sul computer, che il computer non sia in stop e che entrambi i dispositivi siano sulla stessa rete Wi-Fi.",
            "A questo indirizzo ha risposto qualcosa che non è Universal Screens, oppure un firewall blocca la connessione. Controlla indirizzo e porta, oppure scansiona il codice QR del computer.",
            "Il computer ha chiuso la connessione mentre veniva stabilita. Riprova.",
            "Questo PIN non corrisponde a quello mostrato sul computer. Controllalo e riprova.",
            "Troppi PIN errati. Il computer è in pausa; riprova tra %d s.",
            "Questo computer richiede il suo PIN. Scansiona il suo codice QR o inserisci il PIN che mostra.",
            "A questo indirizzo ha risposto un altro computer. Scansiona il codice QR del computer che vuoi usare o inserisci il suo PIN.",
            "Il computer usa una versione precedente di Universal Screens. Aggiornala e riprova.",
            "Il computer usa una versione diversa di Universal Screens. Aggiorna entrambe le app e riprova.",
        ]
        case "de": [
            "Prüfen Sie, ob Universal Screens auf dem Computer geöffnet ist, ob die PIN stimmt und ob beide Geräte im selben WLAN sind.",
            "Diese Adresse wurde nicht gefunden. Prüfen Sie sie, oder scannen Sie stattdessen den QR-Code des Computers.",
            "Unter dieser Adresse hat nichts geantwortet. Prüfen Sie, ob Universal Screens auf dem Computer geöffnet ist, ob der Computer wach ist und ob beide Geräte im selben WLAN sind.",
            "Unter dieser Adresse hat etwas geantwortet, aber nicht Universal Screens, oder eine Firewall blockiert die Verbindung. Prüfen Sie Adresse und Port, oder scannen Sie den QR-Code des Computers.",
            "Der Computer hat die Verbindung beim Aufbau getrennt. Versuchen Sie es erneut.",
            "Diese PIN stimmt nicht mit der auf dem Computer angezeigten überein. Prüfen Sie sie und versuchen Sie es erneut.",
            "Zu viele falsche PINs. Der Computer pausiert; versuchen Sie es in %d s erneut.",
            "Dieser Computer braucht seine PIN. Scannen Sie seinen QR-Code, oder geben Sie die angezeigte PIN ein.",
            "Unter dieser Adresse hat ein anderer Computer geantwortet. Scannen Sie den QR-Code des gewünschten Computers, oder geben Sie seine PIN ein.",
            "Auf dem Computer läuft eine ältere Version von Universal Screens. Aktualisieren Sie sie und versuchen Sie es erneut.",
            "Auf dem Computer läuft eine andere Version von Universal Screens. Aktualisieren Sie beide Apps und versuchen Sie es erneut.",
        ]
        case "pt-BR": [
            "Verifique se o Universal Screens está aberto no computador, se o PIN está certo e se os dois aparelhos estão na mesma rede Wi-Fi.",
            "Esse endereço não foi encontrado. Confira o endereço ou escaneie o código QR do computador.",
            "Nada respondeu nesse endereço. Verifique se o Universal Screens está aberto no computador, se o computador não está em repouso e se os dois aparelhos estão na mesma rede Wi-Fi.",
            "Algo respondeu nesse endereço, mas não é o Universal Screens, ou um firewall está bloqueando a conexão. Confira o endereço e a porta ou escaneie o código QR do computador.",
            "O computador encerrou a conexão enquanto ela era estabelecida. Tente de novo.",
            "Esse PIN não corresponde ao que aparece no computador. Confira e tente de novo.",
            "PINs incorretos demais. O computador está em pausa; tente de novo em %d s.",
            "Este computador precisa do PIN dele. Escaneie o código QR ou digite o PIN que aparece nele.",
            "Outro computador respondeu nesse endereço. Escaneie o código QR do computador que você quer usar ou digite o PIN dele.",
            "O computador está com uma versão antiga do Universal Screens. Atualize e tente de novo.",
            "O computador está com outra versão do Universal Screens. Atualize os dois aplicativos e tente de novo.",
        ]
        case "pt-PT": [
            "Verifique se o Universal Screens está aberto no computador, se o PIN está correto e se os dois dispositivos estão na mesma rede Wi-Fi.",
            "Não foi possível encontrar esse endereço. Verifique-o ou leia o código QR do computador.",
            "Nada respondeu nesse endereço. Verifique se o Universal Screens está aberto no computador, se o computador não está em suspensão e se os dois dispositivos estão na mesma rede Wi-Fi.",
            "Algo respondeu nesse endereço, mas não é o Universal Screens, ou uma firewall está a bloquear a ligação. Verifique o endereço e a porta, ou leia o código QR do computador.",
            "O computador fechou a ligação enquanto esta era estabelecida. Tente novamente.",
            "Esse PIN não corresponde ao que o computador mostra. Verifique-o e tente novamente.",
            "Demasiados PIN errados. O computador está em pausa; tente novamente daqui a %d s.",
            "Este computador precisa do respetivo PIN. Leia o código QR ou introduza o PIN que ele mostra.",
            "Respondeu outro computador nesse endereço. Leia o código QR do computador que pretende usar ou introduza o respetivo PIN.",
            "O computador tem uma versão antiga do Universal Screens. Atualize-a e tente novamente.",
            "O computador tem outra versão do Universal Screens. Atualize as duas aplicações e tente novamente.",
        ]
        case "tr": [
            "Universal Screens'in bilgisayarda açık olduğunu, PIN'in doğru olduğunu ve iki cihazın da aynı Wi-Fi ağında olduğunu kontrol edin.",
            "Bu adres bulunamadı. Adresi kontrol edin ya da bilgisayarın QR kodunu tarayın.",
            "Bu adreste yanıt veren olmadı. Universal Screens'in bilgisayarda açık olduğunu, bilgisayarın uyku modunda olmadığını ve iki cihazın da aynı Wi-Fi ağında olduğunu kontrol edin.",
            "Bu adreste bir şey yanıt verdi ama Universal Screens değil ya da bir güvenlik duvarı bağlantıyı engelliyor. Adresi ve bağlantı noktasını kontrol edin ya da bilgisayarın QR kodunu tarayın.",
            "Bilgisayar, bağlantı kurulurken bağlantıyı kapattı. Yeniden deneyin.",
            "Bu PIN, bilgisayarda görünenle eşleşmiyor. Kontrol edip yeniden deneyin.",
            "Çok fazla yanlış PIN girildi. Bilgisayar bekliyor; %d sn sonra yeniden deneyin.",
            "Bu bilgisayar PIN'ini istiyor. QR kodunu tarayın ya da gösterdiği PIN'i girin.",
            "Bu adreste başka bir bilgisayar yanıt verdi. Kullanmak istediğiniz bilgisayarın QR kodunu tarayın ya da PIN'ini girin.",
            "Bilgisayarda Universal Screens'in eski bir sürümü çalışıyor. Güncelleyip yeniden deneyin.",
            "Bilgisayarda Universal Screens'in farklı bir sürümü çalışıyor. İki uygulamayı da güncelleyip yeniden deneyin.",
        ]
        default: [
            "Check that Universal Screens is open on the computer, that the PIN matches, and that both devices are on the same Wi-Fi.",
            "That address can't be found. Check it, or scan the computer's QR code instead.",
            "Nothing answered at that address. Check that Universal Screens is open on the computer, that the computer is awake, and that both devices are on the same Wi-Fi.",
            "Something answered at that address, but it isn't Universal Screens, or a firewall is blocking the connection. Check the address and port, or scan the computer's QR code.",
            "The computer closed the connection while it was being set up. Try again.",
            "That PIN doesn't match the one the computer shows. Check it and try again.",
            "Too many wrong PINs. The computer is pausing; try again in %d s.",
            "This computer needs its PIN. Scan its QR code, or enter the PIN it shows.",
            "A different computer answered at that address. Scan the QR code of the computer you want, or enter its PIN.",
            "The computer is running an older version of Universal Screens. Update it, then try again.",
            "The computer is running a different version of Universal Screens. Update both apps, then try again.",
        ]
        }
    }
}
