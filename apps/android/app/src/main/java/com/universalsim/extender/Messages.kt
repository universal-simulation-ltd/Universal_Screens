package com.universalsim.extender

/**
 * Why a connect failed — the Rust core's `ConnectProblem`, carried across JNI
 * as a number (`ExtenderNative.nativeLastConnectProblem`). The numbers are the
 * core's and never change meaning; an unknown one reads as [OTHER].
 */
enum class ConnectProblem(val code: Int) {
    OTHER(1),
    BAD_ADDRESS(2),
    UNREACHABLE(3),
    NO_ANSWER(4),
    DROPPED(5),
    WRONG_CODE(6),
    HOST_LOCKED(7),
    NEEDS_CODE(8),
    UNKNOWN_HOST(9),
    OLDER_HOST(10),
    UNSUPPORTED(11);

    companion object {
        fun from(code: Int): ConnectProblem = values().firstOrNull { it.code == code } ?: OTHER
    }
}

/** A failed connect: why, and (for [ConnectProblem.HOST_LOCKED]) how long to wait. */
data class ConnectFailure(val problem: ConnectProblem, val waitSeconds: Int = 0)

/**
 * The sentences the app shows for things the Rust core reports, in the
 * suite's eight languages. The language is the one the knowledge base uses
 * ([Knowledge.language]: the reader's saved choice, else the device's), so a
 * person who reads the help in French also gets French here.
 *
 * ⚠️ No Rust error text ever reaches the screen: it is English, technical
 * ("os error 61", "pairing file: bad identity") and says nothing about what
 * to do. Every problem the core can report has its own sentence below.
 */
object Messages {
    /** The title and explanation for a failed connect. */
    data class Failure(val title: String, val body: String)

    fun connectFailure(lang: String, failure: ConnectFailure?): Failure {
        val f = failure ?: ConnectFailure(ConnectProblem.OTHER)
        val t = table(lang)
        val body = when (f.problem) {
            ConnectProblem.OTHER -> t[0]
            ConnectProblem.BAD_ADDRESS -> t[1]
            ConnectProblem.UNREACHABLE -> t[2]
            ConnectProblem.NO_ANSWER -> t[3]
            ConnectProblem.DROPPED -> t[4]
            ConnectProblem.WRONG_CODE -> t[5]
            ConnectProblem.HOST_LOCKED -> t[6].replace("%d", f.waitSeconds.coerceAtLeast(1).toString())
            ConnectProblem.NEEDS_CODE -> t[7]
            ConnectProblem.UNKNOWN_HOST -> t[8]
            ConnectProblem.OLDER_HOST -> t[9]
            ConnectProblem.UNSUPPORTED -> t[10]
        }
        return Failure(title(lang), body)
    }

    private fun title(lang: String): String = when (lang) {
        "fr" -> "Échec de la connexion"
        "es" -> "No se ha podido conectar"
        "it" -> "Connessione non riuscita"
        "de" -> "Verbindung fehlgeschlagen"
        "pt-BR" -> "Falha na conexão"
        "pt-PT" -> "Falha na ligação"
        "tr" -> "Bağlantı kurulamadı"
        else -> "Connection failed"
    }

    /** One sentence per [ConnectProblem], in code order (OTHER first). */
    private fun table(lang: String): List<String> = when (lang) {
        "fr" -> listOf(
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
        )
        "es" -> listOf(
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
        )
        "it" -> listOf(
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
        )
        "de" -> listOf(
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
        )
        "pt-BR" -> listOf(
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
        )
        "pt-PT" -> listOf(
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
        )
        "tr" -> listOf(
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
        )
        else -> listOf(
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
        )
    }

    /** The failure screen's two buttons. */
    data class FailureButtons(val retry: String, val home: String)

    fun failureButtons(lang: String): FailureButtons = when (lang) {
        "fr" -> FailureButtons("Réessayer", "Retour à l'accueil")
        "es" -> FailureButtons("Reintentar", "Volver al inicio")
        "it" -> FailureButtons("Riprova", "Torna alla home")
        "de" -> FailureButtons("Erneut versuchen", "Zurück zur Startseite")
        "pt-BR" -> FailureButtons("Tentar de novo", "Voltar ao início")
        "pt-PT" -> FailureButtons("Tentar novamente", "Voltar ao início")
        "tr" -> FailureButtons("Yeniden dene", "Ana sayfaya dön")
        else -> FailureButtons("Retry", "Back to home")
    }

    /** "Forget this computer" on a saved row, and its confirmation. */
    data class Forget(val menu: String, val title: String, val body: String, val confirm: String, val cancel: String)

    fun forget(lang: String): Forget = when (lang) {
        "fr" -> Forget("Oublier cet ordinateur", "Oublier cet ordinateur ?", "Ce téléphone demandera son code PIN à la prochaine connexion.", "Oublier", "Annuler")
        "es" -> Forget("Olvidar este ordenador", "¿Olvidar este ordenador?", "Este teléfono le pedirá su PIN la próxima vez que se conecte.", "Olvidar", "Cancelar")
        "it" -> Forget("Dimentica questo computer", "Dimenticare questo computer?", "Questo telefono chiederà il suo PIN alla prossima connessione.", "Dimentica", "Annulla")
        "de" -> Forget("Diesen Computer vergessen", "Diesen Computer vergessen?", "Dieses Handy fragt beim nächsten Verbinden nach seiner PIN.", "Vergessen", "Abbrechen")
        "pt-BR" -> Forget("Esquecer este computador", "Esquecer este computador?", "Este celular vai pedir o PIN dele na próxima vez que você se conectar.", "Esquecer", "Cancelar")
        "pt-PT" -> Forget("Esquecer este computador", "Esquecer este computador?", "Este telemóvel vai pedir o respetivo PIN da próxima vez que ligar.", "Esquecer", "Cancelar")
        "tr" -> Forget("Bu bilgisayarı unut", "Bu bilgisayar unutulsun mu?", "Bu telefon bir sonraki bağlantıda bilgisayarın PIN'ini soracak.", "Unut", "İptal")
        else -> Forget("Forget this computer", "Forget this computer?", "This phone will ask for its PIN next time you connect.", "Forget", "Cancel")
    }
}
