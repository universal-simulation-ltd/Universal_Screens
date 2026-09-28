Universal Screens knowledge base, German. Same articles, ids and order as en.md; see en.md for the format. The app's interface is in English, so the names of its buttons and menus are kept as they appear on screen.

---
id: what-is-screens
group: Grundlagen
title: Was ist Universal Screens?
summary: Ein Handy oder ein Browser als Präsentationsfernbedienung, Touchpad, Spiegel, Fernsteuerung oder zusätzlicher Bildschirm für Ihren Computer.
---
Mit Universal Screens kann ein anderes Gerät den Bildschirm, die Maus und die Tastatur Ihres Computers nutzen. Es besteht aus zwei Teilen.

- **Der Host** läuft auf dem Computer, den Sie erreichen möchten: einem Mac, einem Windows-PC oder einem Linux-Rechner. Er zeigt einen QR-Code und eine 4-stellige PIN und nimmt nur Verbindungen an, solange sein Fenster geöffnet ist.
- **Ein Client** ist das, womit Sie sich verbinden: die Handy-App oder ein Webbrowser auf einem anderen Gerät.

## Was Sie damit tun können

- **Clicker:** eine Präsentationsfernbedienung mit Tasten für vor, zurück und schwarzen Bildschirm sowie Vorschauen Ihrer Folien.
- **Trackpad:** Das Handy wird zum Touchpad, um den Zeiger zu bewegen, zu tippen und zu scrollen.
- **Mirror:** Sie sehen den Bildschirm des Computers, ohne ihn zu steuern.
- **Remote control:** Sie sehen den Bildschirm und steuern ihn mit Maus und Tastatur.
- **Second screen:** Das Handy wird zum zusätzlichen Bildschirm. Unter Windows braucht der PC dafür einen installierten Treiber für virtuelle Bildschirme.

Die Handy-App bietet alle fünf. Der Browser-Client bietet Remote control, Mirror und Clicker.

---
id: connecting
group: Grundlagen
title: Wie verbinde ich mich mit meinem Computer?
summary: QR-Code scannen, einen Computer in der Nähe wählen oder Adresse und PIN eintippen.
---
Starten Sie zuerst den Host auf dem Computer. Ihr Gerät und der Computer müssen im selben Netzwerk sein, es sei denn, Sie nutzen einen Fernzugriffscode (siehe unten).

## Den QR-Code scannen

Tippen Sie in der Handy-App auf **Scan to connect** und richten Sie die Kamera auf den QR-Code im Fenster des Hosts. Der Code enthält die Adresse des Computers und die PIN, Sie müssen also nichts eintippen. Er kann auch das WLAN des Computers enthalten: Ab Android 10 bittet die App dann darum, diesem Netzwerk beizutreten, und Android zeigt vorher eine eigene Bestätigung an.

## Computer in der Nähe

Solange der Verbindungsbildschirm geöffnet ist, listet die Handy-App die Hosts auf, die sie in Ihrem Netzwerk findet. Tippen Sie auf einen und geben Sie seine PIN ein, die der Host in seinem Fenster anzeigt.

## Von Hand eingeben

In der Handy-App gibt es unter **Advanced** Felder für die Adresse des Hosts (etwa 192.168.1.20:9000) und seine PIN. Der Browser-Client fragt sie auf seiner Hauptseite ab.

## Gespeicherte Rechner

Jeder Rechner, mit dem Sie sich verbinden, wird gespeichert, beim nächsten Mal genügt also ein Tippen. In der Handy-App können Sie einen gespeicherten Rechner umbenennen, ausblenden oder löschen, und **Remember next time?** erspart Ihnen die Wahl des Modus.

## Aus einem anderen Netzwerk

Auf einem Mac oder Windows-PC gibt Ihnen **Remote access (other networks)** im Fenster des Hosts einen Code. Geben Sie ihn im Browser-Client unter **Remote (across networks)** ein, zusammen mit der PIN des Hosts. Die Verbindung läuft über den Relay-Server von UNI·SIM, rechnen Sie also mit mehr Verzögerung als im eigenen Netzwerk.

---
id: the-pin
group: So funktioniert es
title: Wozu dient die PIN?
summary: Sie lässt Ihre Geräte herein und ist zugleich der Schlüssel, mit dem die Verbindung verschlüsselt wird.
---
Jede Verbindung braucht die 4-stellige PIN des Hosts. Wenn Sie den QR-Code scannen, wird sie automatisch eingetragen.

Die PIN hat zwei Aufgaben. Der Host prüft sie, bevor er ein Gerät hereinlässt, und sie ist zugleich der Schlüssel, mit dem die Verbindung verschlüsselt wird. Der Artikel zur Verschlüsselung erklärt den zweiten Teil.

## Jedes Mal eine neue PIN

Sofern Sie keine eigene wählen, erzeugt der Host bei jedem Start eine neue PIN mit dem sicheren Zufallszahlengenerator des Computers. Auf einem Mac oder Windows-PC wählt **Actions ▸ Regenerate PIN** sofort eine neue.

## Eine eigene wählen

Wenn sich gespeicherte Geräte nach einem Neustart des Computers wieder verbinden sollen, können Sie eine eigene PIN festlegen: **Actions ▸ Use my own PIN** auf einem Mac oder Windows-PC oder unter **⚙** unter Linux. Standardmäßig ist das ausgeschaltet. 0000 ist nicht erlaubt, weil es in der Verbindung „keine PIN“ bedeutet. Eine selbst gewählte PIN gilt, bis Sie sie ändern, für jeden, der sie kennt.

## Falsche Eingaben

Die ersten drei falschen PINs hintereinander kosten nichts, damit ein Tippfehler Sie nicht aussperrt. Danach nimmt der Host 1 Sekunde lang keine Verbindungen an, dann 2, jedes Mal doppelt so lange bis zu 5 Minuten. Die richtige PIN setzt den Zähler zurück, ebenso eine Stunde ohne falsche PIN. Die Pause gilt für alle: Solange jemand weiter rät, müssen auch Ihre eigenen Geräte warten.

## Wer sie kennt

Wer die PIN oder ein Foto des QR-Codes hat, kann sich verbinden und den Computer steuern. Eine eigene Freigabe pro Gerät gibt es nicht. Nachdem Sie Ihren Bildschirm mit jemandem geteilt haben, erzeugen Sie eine neue PIN.

---
id: encryption
group: So funktioniert es
title: Ist die Verbindung verschlüsselt?
summary: Ja, Ende-zu-Ende, mit Ihrer PIN als Schlüssel. Der Browser-Client sagt es Ihnen, wenn das nicht möglich ist.
---
Ja. Sobald Ihr Gerät den Host erreicht, führen beide einen Handshake aus dem Noise Protocol Framework aus, einem veröffentlichten Entwurf für verschlüsselte Verbindungen. Alles danach läuft durch den verschlüsselten Tunnel: das Bild des Bildschirms, Ihre Tastenanschläge und Texte und auch die PIN-Prüfung selbst.

## Welche Rolle die PIN spielt

Der Handshake bezieht die PIN als gemeinsames Geheimnis ein. Ein Gerät mit der falschen PIN kann ihn nicht abschließen, und ebenso wenig jemand, der sich ohne die PIN in die Verbindung einschalten will. Jede Verbindung erzeugt außerdem neue Einmalschlüssel, sodass ein Mitschnitt des Datenverkehrs unlesbar bleibt, selbst wenn die PIN später bekannt wird.

## Über den Relay-Server

Mit einem Fernzugriffscode läuft die Verbindung über den Relay-Server von UNI·SIM. Browser und Host verschlüsseln trotzdem Ende-zu-Ende, der Server reicht also nur verschlüsselte Daten weiter, die er nicht lesen kann.

## Wann sie nicht verschlüsselt ist

Versionen aus der Zeit vor der Verschlüsselung verbinden sich weiterhin ohne sie. Der Host protokolliert eine Warnung, wenn sich ein alter Client unverschlüsselt verbindet. Der Browser-Client prüft, was der Host kann, und vermerkt im Sitzungsprotokoll, ob die Sitzung Ende-zu-Ende verschlüsselt ist. Ist der Host eine ältere Version, steht das im Protokoll, zusammen mit der Bitte, ihn zu aktualisieren.

## Für technisch Interessierte

Das Muster ist Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s, mit einem aus der PIN abgeleiteten Schlüssel als Pre-Shared Key.

---
id: what-leaves-your-network
group: Datenschutz und Sicherheit
title: Was verlässt Ihr Netzwerk?
summary: Ihr Bildschirm geht nur an das gekoppelte Gerät. Hier ist alles andere, und wohin es geht.
---
Ihr Bildschirm wird nie zu UNI·SIM hochgeladen. In Ihrem eigenen Netzwerk geht er direkt vom Host an das gekoppelte Gerät und nirgendwo anders hin.

## Was in Ihrem Netzwerk bleibt

- Das Bild des Bildschirms, Ihre Tastenanschläge und Ihre Maus- und Toucheingaben.
- Die Ankündigung des Hosts an Geräte in der Nähe, mit seinem Namen und Port, damit Handys im selben Netzwerk ihn auflisten können.
- Die Liste der letzten Verbindungen des Hosts, die Sie über sein Menü Actions löschen können.

## Was an UNI·SIM geht, und wann

- **Die Nutzerzahl.** Screens sendet eine zufällige Installations-ID, um anzeigen zu können, wie viele Menschen es nutzen. Wenn Sie angemeldet sind, nennt sie auch das Konto, damit Ihre Geräte als eine Person zählen. Nichts über Ihren Bildschirm, Ihre PIN oder Ihre Rechner ist Teil davon.
- **Ihre Universal ID, wenn Sie sich anmelden.** Der Artikel zur Anmeldung nennt genau, was gespeichert wird.
- **Fernzugriff.** Eine Sitzung mit Fernzugriffscode läuft über den Relay-Server von UNI·SIM. Sie ist Ende-zu-Ende verschlüsselt, der Server kann sie also nicht lesen.
- **Cast to a browser screen.** Wenn die Handy-App auf diese Weise einen Browser-Tab steuert, laufen ihre Touchpad- und Clicker-Befehle über denselben Server. Sie sind auf dem Weg geschützt, aber nicht Ende-zu-Ende verschlüsselt. Es wird kein Bild Ihres Bildschirms gesendet.

In einem Netzwerk ohne Internetverbindung wird nichts davon gesendet, und Screens funktioniert genauso.

## Mit der Handykamera scannen

Der QR-Code des Hosts ist eine Webadresse. Wenn Sie ihn mit der Kamera des Handys scannen und die App nicht installiert ist, öffnet der Browser diese Seite, und die lokale Adresse des Hosts sowie seine PIN sind Teil der Adresse, die die Website erhält. Ein WLAN-Passwort, falls der Code eines enthält, steht in dem Teil der Adresse, den Browser nie senden.

---
id: signing-in
group: Datenschutz und Sicherheit
title: Was bringt die Anmeldung?
summary: Sie ist freiwillig. Angemeldet folgen Ihnen Ihre gespeicherten Rechner, Ihre PINs aber nie.
---
Zum Verbinden brauchen Sie kein Konto. Die Anmeldung mit Ihrer Universal ID dient nur dazu, dass ein neues Handy oder ein neuer Browser Ihre Rechner bereits kennt.

## Was synchronisiert wird

Die Handy-App und der Browser-Client speichern Ihre Rechner bei Ihrem Konto. Für jeden sind das:

- seine Adresse in Ihrem Netzwerk
- der eigene Name und das Betriebssystem des Rechners
- der Name, den Sie ihm gegeben haben, falls vorhanden
- wann Sie zuletzt verbunden waren

Wenn Sie einen gespeicherten Rechner angemeldet löschen, wird er auch aus Ihrem Konto entfernt.

## Was nicht

- **Die PIN.** Sie ist der Schlüssel, mit dem die Verbindung verschlüsselt wird, und wird deshalb nie gesendet. Die Handy-App speichert sie mit jedem Rechner auf dem Handy, um sich wieder verbinden zu können. Der Browser-Client speichert sie überhaupt nicht.
- In der Handy-App bleiben der gewählte Modus und die Frage, ob Sie einen Rechner ausgeblendet haben, auf dem Handy.
- Alles, was Ihren Bildschirm betrifft.

## Auf dem Computer

Der Host meldet sich nur zur Identifizierung an. Er hat keine Liste von Rechnern zu synchronisieren, denn er ist selbst der Rechner: Seine letzten Verbindungen gehören zu diesem Computer, nicht zu Ihnen. Eine Anmeldung dort lässt die Nutzerzahl Ihren Computer, Ihr Handy und Ihren Browser als eine Person zählen.

## Wo Konten angelegt werden

Screens kann sich nur bei einer bereits bestehenden Universal ID anmelden. Sie legen sie kostenlos auf app.unisim.co.uk an, sodass eine vertippte E-Mail-Adresse nie versehentlich ein Konto erzeugt.

---
id: email-code-sign-in
group: Datenschutz und Sicherheit
title: Warum gibt es kein „Mit Google anmelden“?
summary: Screens meldet Sie mit einem per E-Mail gesendeten Code an, der einzigen Methode, die in jedem Client funktioniert.
---
Zum Anmelden geben Sie Ihre E-Mail-Adresse ein, erhalten einen 6-stelligen Code und tippen ihn ein. Es gibt kein Passwort.

## Warum nicht Google

Der Browser-Client wird von einem Rechner in Ihrem eigenen Netzwerk über eine einfache http://-Adresse geöffnet. Anders geht es nicht: Browser verhindern, dass eine sichere https://-Seite die Art lokaler Verbindung öffnet, über die der Client den Host erreicht. Google erlaubt seine Anmeldung aber nur auf sicheren Webadressen, die vorab bei Google registriert wurden, und eine Adresse in Ihrem Heim- oder Büronetzwerk kann das nie sein. Deshalb nutzt Screens überall den Code per E-Mail: in der Handy-App, im Browser und auf dem Computer.

## Nach der Anmeldung

Sie bleiben auf diesem Gerät angemeldet, bis Sie sich abmelden. Kann die Anmeldung nicht mehr erneuert werden, etwa weil das Konto gelöscht wurde, meldet die App Sie auf diesem Gerät ab. Die Verbindung zu Ihren Rechnern hängt nie davon ab.
