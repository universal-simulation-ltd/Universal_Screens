Universal Screens knowledge base, Italian. Same articles, ids and order as en.md; see en.md for the format. The app's interface is in English, so the names of its buttons and menus are kept as they appear on screen.

---
id: what-is-screens
group: Le basi
title: Che cos'è Universal Screens?
summary: Un telefono o un browser come telecomando, touchpad, specchio, controllo remoto o schermo in più per il tuo computer.
---
Universal Screens permette a un altro dispositivo di usare lo schermo, il mouse e la tastiera del tuo computer. È fatta di due parti.

- **L'host** gira sul computer che vuoi raggiungere: un Mac, un PC Windows o una macchina Linux. Mostra un codice QR e un PIN di 4 cifre, e accetta connessioni solo finché la sua finestra è aperta.
- **Un client** è il dispositivo da cui ti colleghi: l'app per il telefono, oppure un browser web su un altro dispositivo.

## Cosa puoi farci

- **Clicker:** un telecomando per presentazioni con i pulsanti avanti, indietro e schermo nero, e le anteprime delle tue slide.
- **Trackpad:** usa il telefono come touchpad per muovere il puntatore, toccare e scorrere.
- **Mirror:** guarda lo schermo del computer, senza controllarlo.
- **Remote control:** vedi lo schermo e controllalo con mouse e tastiera.
- **Second screen:** usa il telefono come schermo aggiuntivo. Su Windows serve un driver per display virtuali installato sul PC.

L'app per il telefono le offre tutte e cinque. Il client browser offre Remote control, Mirror e Clicker.

---
id: connecting
group: Le basi
title: Come mi collego al mio computer?
summary: Scansiona il codice QR, scegli un computer nelle vicinanze oppure digita il suo indirizzo e il PIN.
---
Avvia prima l'host sul computer. Il tuo dispositivo e il computer devono essere sulla stessa rete, a meno che tu non usi un codice remoto (vedi sotto).

## Scansiona il codice QR

Nell'app per il telefono, tocca **Scan to connect** e inquadra con la fotocamera il codice QR nella finestra dell'host. Il codice contiene l'indirizzo del computer e il PIN, quindi non devi digitare nulla. Può contenere anche la rete Wi-Fi del computer: su Android 10 e versioni successive, l'app chiede allora di collegarsi a quella rete, e prima Android mostra la propria richiesta di conferma.

## Computer nelle vicinanze

Finché la schermata di connessione è aperta, l'app per il telefono elenca gli host che trova sulla tua rete. Toccane uno e digita il suo PIN, che l'host mostra nella sua finestra.

## Inserimento manuale

Nell'app per il telefono, **Advanced** ha i campi per l'indirizzo dell'host (per esempio 192.168.1.20:9000) e il suo PIN. Il client browser li chiede nella sua pagina principale.

## Computer salvati

Ogni computer a cui ti colleghi viene salvato, così la volta dopo basta un tocco. Nell'app per il telefono puoi rinominare, nascondere o eliminare un computer salvato, e **Remember next time?** ti evita di scegliere la modalità.

## Da un'altra rete

Su un Mac o un PC Windows, **Remote access (other networks)** nella finestra dell'host ti dà un codice. Inseriscilo nel client browser sotto **Remote (across networks)**, insieme al PIN dell'host. La connessione passa dal server di inoltro di UNI·SIM, quindi aspettati più ritardo che sulla tua rete.

---
id: the-pin
group: Come funziona
title: A cosa serve il PIN?
summary: Fa entrare i tuoi dispositivi ed è anche la chiave con cui viene cifrata la connessione.
---
Ogni connessione richiede il PIN di 4 cifre dell'host. Scansionando il codice QR viene inserito per te.

Il PIN ha due compiti. L'host lo controlla prima di far entrare un dispositivo, ed è anche la chiave con cui viene cifrata la connessione. L'articolo sulla cifratura spiega la seconda parte.

## Un PIN nuovo ogni volta

A meno che tu non scelga il tuo, l'host genera un PIN nuovo a ogni avvio, dal generatore sicuro di numeri casuali del computer. Su un Mac o un PC Windows, **Actions ▸ Regenerate PIN** ne sceglie subito uno nuovo.

## Scegliere il tuo

Se vuoi che i dispositivi salvati si ricolleghino dopo il riavvio del computer, puoi impostare un PIN tuo: **Actions ▸ Use my own PIN** su un Mac o un PC Windows, oppure sotto **⚙** su Linux. È disattivato per impostazione predefinita. 0000 non è ammesso, perché nella connessione significa «nessun PIN». Un PIN scelto da te resta valido finché non lo cambi, per chiunque lo abbia scoperto.

## Tentativi sbagliati

I primi tre PIN sbagliati di fila non costano nulla, così un errore di battitura non ti blocca. Dopo, l'host smette di accettare connessioni per 1 secondo, poi 2, raddoppiando ogni volta fino a 5 minuti. Il PIN giusto azzera il conteggio, e lo stesso fa un'ora senza errori. La pausa vale per tutti: finché qualcuno continua a tentare, anche i tuoi dispositivi devono aspettare.

## Chi lo conosce

Chiunque abbia il PIN, o una foto del codice QR, può collegarsi e controllare il computer. Non c'è un'approvazione separata per ogni dispositivo. Dopo aver condiviso lo schermo con qualcuno, genera un PIN nuovo.

---
id: encryption
group: Come funziona
title: La connessione è cifrata?
summary: Sì, end-to-end, con il tuo PIN come chiave. Il client browser ti avvisa se non è possibile.
---
Sì. Appena il tuo dispositivo raggiunge l'host, i due eseguono uno scambio iniziale del Noise Protocol Framework, un progetto pubblico per connessioni cifrate. Tutto ciò che segue viaggia nel tunnel cifrato: l'immagine dello schermo, i tasti premuti e il testo, e lo stesso controllo del PIN.

## Il ruolo del PIN

Lo scambio iniziale usa il PIN come segreto condiviso. Un dispositivo con il PIN sbagliato non riesce a completarlo, e nemmeno chi cerca di mettersi in mezzo alla connessione senza il PIN. Ogni connessione crea inoltre chiavi nuove e usa e getta, quindi una registrazione del traffico resta illeggibile anche se il PIN viene scoperto in seguito.

## Attraverso il server di inoltro

Con un codice remoto, la connessione passa dal server di inoltro di UNI·SIM. Il browser e l'host cifrano comunque end-to-end, quindi il server inoltra solo dati cifrati che non può leggere.

## Quando non è cifrata

Le versioni precedenti alla cifratura si collegano ancora senza. L'host registra un avviso quando un vecchio client si collega in chiaro. Il client browser verifica cosa supporta l'host e indica nel registro della sessione se è cifrata end-to-end. Se l'host è una versione vecchia, il registro lo dice e ti chiede di aggiornarlo.

## Per chi ama i dettagli tecnici

Lo schema è Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s, con una chiave derivata dal PIN come chiave precondivisa.

---
id: what-leaves-your-network
group: Privacy e sicurezza
title: Cosa esce dalla tua rete?
summary: Il tuo schermo va solo al dispositivo abbinato. Ecco tutto il resto, e dove va.
---
Il tuo schermo non viene mai caricato su UNI·SIM. Sulla tua rete va direttamente dall'host al dispositivo che hai abbinato, e da nessun'altra parte.

## Cosa resta sulla tua rete

- L'immagine dello schermo, i tasti premuti e quello che fai con il mouse o al tocco.
- L'annuncio dell'host ai dispositivi vicini, che contiene il suo nome e la porta perché i telefoni sulla stessa rete possano elencarlo.
- L'elenco delle connessioni recenti dell'host, che puoi cancellare dal suo menu Actions.

## Cosa va a UNI·SIM, e quando

- **Il conteggio degli utenti.** Screens invia un ID di installazione casuale, per poter mostrare quante persone lo usano. Se hai effettuato l'accesso, indica anche l'account, così i tuoi dispositivi contano come una sola persona. Non contiene nulla sul tuo schermo, sul PIN o sui tuoi computer.
- **Il tuo Universal ID, se accedi.** L'articolo sull'accesso elenca esattamente cosa viene conservato.
- **L'accesso remoto.** Una sessione con codice remoto passa dal server di inoltro di UNI·SIM. È cifrata end-to-end, quindi il server non può leggerla.
- **Cast to a browser screen.** Quando l'app per il telefono comanda così una scheda del browser, i suoi comandi di touchpad e telecomando passano dallo stesso server. Sono protetti lungo il percorso, ma non sono cifrati end-to-end. Nessuna immagine del tuo schermo viene inviata.

Su una rete senza connessione a Internet non viene inviato nulla di tutto questo, e Screens funziona esattamente allo stesso modo.

## Scansione con la fotocamera del telefono

Il codice QR dell'host è un indirizzo web. Se lo scansioni con la fotocamera del telefono e l'app non è installata, il browser apre quella pagina, e l'indirizzo locale dell'host e il suo PIN fanno parte dell'indirizzo che riceve il sito. Una password Wi-Fi, se il codice ne contiene una, si trova nella parte dell'indirizzo che i browser non inviano mai.

---
id: signing-in
group: Privacy e sicurezza
title: A cosa serve accedere?
summary: È facoltativo. Con l'accesso, i tuoi computer salvati ti seguono, ma i PIN mai.
---
Per collegarti ai tuoi computer non serve nessun account. Accedere con il tuo Universal ID serve solo perché un telefono o un browser nuovo conosca già i tuoi computer.

## Cosa viene sincronizzato

L'app per il telefono e il client browser conservano i tuoi computer salvati con il tuo account. Per ciascuno si tratta di:

- il suo indirizzo sulla tua rete
- il nome e il sistema operativo del computer
- il nome che gli hai dato, se c'è
- quando ti sei collegato l'ultima volta

Se elimini un computer salvato mentre hai effettuato l'accesso, viene tolto anche dal tuo account.

## Cosa no

- **Il PIN.** È la chiave con cui viene cifrata la connessione, quindi non viene mai inviato. L'app per il telefono lo tiene sul telefono insieme a ogni computer salvato, per potersi ricollegare. Il client browser non lo salva affatto.
- Nell'app per il telefono, la modalità scelta e il fatto di aver nascosto un computer restano sul telefono.
- Niente che riguardi il tuo schermo.

## Sul computer

L'host accede solo per identificarti. Non ha un elenco di computer da sincronizzare, perché è lui stesso il computer: le sue connessioni recenti appartengono a quel computer, non a te. Accedere lì permette al conteggio degli utenti di considerare computer, telefono e browser come una sola persona.

## Dove si creano gli account

Screens può accedere solo a un Universal ID che esiste già. Ne crei uno, gratis, su app.unisim.co.uk, così un indirizzo email scritto male non può mai creare un account per sbaglio.

---
id: email-code-sign-in
group: Privacy e sicurezza
title: Perché non c'è «Accedi con Google»?
summary: Screens ti fa accedere con un codice inviato via email, l'unico metodo che funziona in tutti i client.
---
Per accedere, digiti il tuo indirizzo email, ricevi un codice di 6 cifre e lo digiti. Non c'è nessuna password.

## Perché non Google

Il client browser si apre da un computer della tua rete, a un semplice indirizzo http://. Non può essere altrimenti: i browser impediscono a una pagina sicura https:// di aprire il tipo di connessione locale che il client usa per raggiungere l'host. Google consente il suo accesso solo su indirizzi web sicuri registrati in anticipo, e un indirizzo della rete di casa o dell'ufficio non potrà mai esserlo. Per questo Screens usa il codice via email ovunque: nell'app per il telefono, nel browser e sul computer.

## Dopo l'accesso

Resti collegato su quel dispositivo finché non esci. Se l'accesso non può più essere rinnovato, per esempio perché l'account è stato eliminato, l'app ti fa uscire su quel dispositivo. La connessione ai tuoi computer non dipende mai da questo.
