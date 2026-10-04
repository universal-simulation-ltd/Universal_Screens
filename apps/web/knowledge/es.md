Universal Screens knowledge base, Spanish. Same articles, ids and order as en.md; see en.md for the format. The app's interface is in English, so the names of its buttons and menus are kept as they appear on screen.

---
id: what-is-screens
group: Lo básico
title: ¿Qué es Universal Screens?
summary: Un teléfono o un navegador como mando, panel táctil, espejo, control remoto o pantalla adicional para su ordenador.
---
Universal Screens permite que otro dispositivo use la pantalla, el ratón y el teclado de su ordenador. Tiene dos partes.

- **El host** se ejecuta en el ordenador al que quiere acceder: un Mac, un PC con Windows o un equipo con Linux. Muestra un código QR y un PIN de 4 dígitos, y solo acepta conexiones mientras su ventana está abierta.
- **Un cliente** es desde donde usted se conecta: la aplicación para el teléfono o un navegador web en otro dispositivo.

## Qué puede hacer con él

- **Clicker:** un mando de presentaciones con botones de siguiente, anterior y pantalla en negro, y vistas previas de sus diapositivas.
- **Trackpad:** use el teléfono como panel táctil para mover el puntero, tocar y desplazarse.
- **Mirror:** vea la pantalla del ordenador, sin controlarla.
- **Remote control:** vea la pantalla y contrólela con el ratón y el teclado.
- **Second screen:** use el teléfono como pantalla adicional. En Windows, esto requiere un controlador de pantalla virtual instalado en el PC.

La aplicación para el teléfono ofrece los cinco. El cliente de navegador ofrece Remote control, Mirror y Clicker.

---
id: connecting
group: Lo básico
title: ¿Cómo me conecto a mi ordenador?
summary: Escanee el código QR, elija un ordenador cercano o escriba su dirección y su PIN.
---
Inicie primero el host en el ordenador. Su dispositivo y el ordenador deben estar en la misma red, salvo que use un código remoto (véase más abajo).

## Escanear el código QR

En la aplicación para el teléfono, toque **Scan to connect** y apunte la cámara al código QR de la ventana del host. El código contiene la dirección del ordenador y su PIN, así que no tiene que escribir nada. También puede contener la red Wi-Fi del ordenador: en Android 10 o posterior, la aplicación pide entonces unirse a esa red, y Android muestra antes su propio aviso de confirmación.

## Ordenadores cercanos

Mientras la pantalla de conexión está abierta, la aplicación para el teléfono muestra los hosts que encuentra en su red. Toque uno y escriba su PIN, que el host muestra en su ventana.

## Escribirlo a mano

En la aplicación para el teléfono, **Advanced** tiene campos para la dirección del host (por ejemplo, 192.168.1.20:9000) y su PIN. El cliente de navegador los pide en su página principal.

## Equipos guardados

Cada equipo al que se conecta queda guardado, así que la próxima vez basta con un toque. En la aplicación para el teléfono puede cambiar el nombre de un equipo guardado, ocultarlo o eliminarlo, y **Remember next time?** le ahorra elegir el modo.

## Desde otra red

En un Mac o un PC con Windows, **Remote access (other networks)** en la ventana del host le da un código. Introdúzcalo en el cliente de navegador en **Remote (across networks)**, junto con el PIN del host. La conexión pasa por el servidor de retransmisión de UNI·SIM, así que habrá más retardo que en su propia red.

---
id: the-pin
group: Cómo funciona
title: ¿Para qué sirve el PIN?
summary: Vincula un dispositivo con el host la primera vez. Después, el dispositivo queda recordado.
---
Un dispositivo necesita el PIN de 4 dígitos del host la primera vez que se conecta. Al escanear el código QR, se rellena solo.

El PIN demuestra que su dispositivo puede entrar, sin enviarse nunca. Una vez vinculado un dispositivo, el dispositivo y el host se recuerdan mutuamente, y vuelve a conectarse sin el PIN, aunque el PIN haya cambiado. El artículo sobre el cifrado explica cómo.

## Un PIN nuevo cada vez

Salvo que elija uno propio, el host genera un PIN nuevo cada vez que se inicia, con el generador seguro de números aleatorios del ordenador. En un Mac o un PC con Windows, **Actions ▸ Regenerate PIN** elige uno nuevo al instante.

## Elegir el suyo

Los dispositivos vinculados vuelven a conectarse sin PIN de todos modos. Un PIN propio le resulta útil si quiere vincular dispositivos nuevos sin leer uno nuevo en la pantalla, y para las aplicaciones de la versión 0.3 y anteriores, que siguen necesitando el PIN actual cada vez. Se fija con **Actions ▸ Use my own PIN** en un Mac o un PC con Windows, o en **⚙** en Linux. Está desactivado de forma predeterminada. No se admite 0000, porque en la conexión significa «sin PIN». Un PIN que usted eligió sigue funcionando hasta que lo cambie, para cualquiera que lo conozca.

## Dispositivos vinculados

**Actions ▸ Paired devices** en un Mac o un PC con Windows, o en **⚙** en Linux, muestra los dispositivos vinculados con el ordenador. **Forget all paired devices** hace que todos ellos tengan que volver a introducir el PIN. Hágalo si pierde un teléfono u ordenador vinculado, o si ya no es suyo.

## Intentos erróneos

Los tres primeros PIN erróneos seguidos no tienen consecuencias, para que una errata no le deje fuera. A partir de ahí, el host deja de aceptar conexiones durante 1 segundo, luego 2, y así duplicando hasta 5 minutos. El PIN correcto pone el contador a cero, igual que una hora sin errores. La pausa se aplica a todos: mientras alguien siga probando, sus propios dispositivos también tendrán que esperar.

## Quién lo tiene

Cualquiera que tenga el PIN, o una foto del código QR, puede vincular un dispositivo y controlar el ordenador, y ese dispositivo sigue vinculado hasta que usted lo olvide. No hay una aprobación aparte para cada dispositivo. Después de compartir su pantalla con alguien, genere un PIN nuevo y olvide los dispositivos vinculados que no reconozca.

---
id: encryption
group: Cómo funciona
title: ¿Está cifrada la conexión?
summary: Sí, de extremo a extremo. Su PIN nunca se envía, y una grabación de la conexión no sirve para averiguarlo.
---
Sí. En cuanto su dispositivo llega al host, ambos realizan un intercambio inicial del Noise Protocol Framework, un diseño publicado para conexiones cifradas. Todo lo que viene después viaja dentro del túnel cifrado: la imagen de la pantalla, y sus pulsaciones de teclas y el texto.

## Qué papel tiene el PIN

La primera vez que un dispositivo se conecta, se vincula con el host mediante el PIN. La vinculación usa SPAKE2, un intercambio de claves autenticado por contraseña: cada lado incorpora el PIN a un intercambio de claves nuevo, de modo que ambos pueden demostrar que conocen el mismo PIN sin enviarlo, ni nada que se pueda calcular solo a partir del PIN. Quien grabe la conexión no puede usar la grabación para adivinar el PIN. Quien pruebe en directo tiene un intento por conexión, y la pausa del host tras los PIN erróneos limita esos intentos.

Durante la vinculación, el host y el dispositivo intercambian claves permanentes y se recuerdan mutuamente. Las conexiones posteriores usan esas claves en lugar del PIN, así que el dispositivo vuelve a conectarse sin él, y quien no tenga una de esas claves no puede interponerse en la conexión. Además, cada conexión crea claves nuevas de un solo uso, así que una grabación sigue siendo ilegible aunque una clave o el PIN lleguen a conocerse más tarde.

## A través del servidor de retransmisión

Con un código remoto, la conexión pasa por el servidor de retransmisión de UNI·SIM. El navegador y el host siguen cifrando de extremo a extremo, así que el servidor solo reenvía datos cifrados que no puede leer.

## Versiones anteriores

La versión 0.3 y las anteriores se vinculaban de otra forma, con una clave calculada solo a partir del PIN. Quien grabara una de esas conexiones podía probar los 10 000 PIN con la grabación y encontrar el suyo. Los hosts actuales siguen dejando entrar a esas aplicaciones antiguas, para que nada deje de funcionar, y registran una advertencia cuando se conecta una. Actualícelas.

Una aplicación actual para el teléfono o el ordenador nunca vuelve al método antiguo: si el host es de la versión 0.3 o anterior, le pide que lo actualice. El cliente de navegador solo usa el método antiguo cuando el host indica que no puede hacer otra cosa, lo dice en el registro de la sesión y nunca lo hace con un host con el que ya se haya vinculado antes con el método actual. Las versiones anteriores al cifrado se conectan sin él, y el registro también lo dice.

## Para quien le interese la parte técnica

La vinculación ejecuta SPAKE2 (grupo Ed25519) sobre el PIN y después Noise_XXpsk0_25519_ChaChaPoly_BLAKE2s, con una clave derivada del resultado de SPAKE2 como clave precompartida. La reconexión ejecuta Noise_XX_25519_ChaChaPoly_BLAKE2s con las claves recordadas. La versión 0.3 y las anteriores usaban Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s, con una clave derivada del PIN como clave precompartida.

---
id: what-leaves-your-network
group: Privacidad y seguridad
title: ¿Qué sale de su red?
summary: Su pantalla solo va al dispositivo que emparejó. Esto es todo lo demás, y adónde va.
---
Su pantalla nunca se sube a UNI·SIM. En su propia red, va directamente del host al dispositivo que emparejó, y a ningún otro sitio.

## Lo que se queda en su red

- La imagen de la pantalla, sus pulsaciones de teclas y lo que hace con el ratón o al tocar.
- El anuncio del host a los dispositivos cercanos, que lleva su nombre y su puerto para que los teléfonos de la misma red puedan mostrarlo.
- La lista de conexiones recientes del host, que puede borrar desde su menú Actions.

## Lo que va a UNI·SIM, y cuándo

- **El recuento de usuarios.** Screens envía un identificador de instalación aleatorio para poder mostrar cuántas personas lo usan. Si ha iniciado sesión, también indica la cuenta, para que sus dispositivos cuenten como una sola persona. No incluye nada sobre su pantalla, su PIN ni sus equipos.
- **Su Universal ID, si inicia sesión.** El artículo sobre el inicio de sesión detalla exactamente qué se guarda.
- **El acceso remoto.** Una sesión con código remoto pasa por el servidor de retransmisión de UNI·SIM. Está cifrada de extremo a extremo, así que el servidor no puede leerla.
- **Cast to a browser screen.** Cuando la aplicación para el teléfono controla así una pestaña del navegador, sus órdenes de panel táctil y de mando pasan por el mismo servidor. Van protegidas por el camino, pero no están cifradas de extremo a extremo. No se envía ninguna imagen de su pantalla.

En una red sin conexión a Internet no se envía nada de esto, y Screens funciona exactamente igual.

## Escanear con la cámara del teléfono

El código QR del host es una dirección web. Si lo escanea con la cámara del propio teléfono y la aplicación no está instalada, el navegador abre esa página, y la dirección local del host y su PIN forman parte de la dirección que recibe el sitio web. Si el código incluye una contraseña Wi-Fi, esta va en la parte de la dirección que los navegadores nunca envían.

---
id: signing-in
group: Privacidad y seguridad
title: ¿Para qué sirve iniciar sesión?
summary: Es opcional. Con la sesión iniciada, sus equipos guardados le acompañan, pero nunca sus PIN.
---
Para conectarse a sus equipos no hace falta ninguna cuenta. Iniciar sesión con su Universal ID solo sirve para que un teléfono o navegador nuevo ya conozca sus equipos.

## Qué se sincroniza

La aplicación para el teléfono y el cliente de navegador guardan sus equipos con su cuenta. De cada uno se guarda:

- su dirección en su red
- el nombre y el sistema operativo del propio equipo
- el nombre que usted le puso, si lo hay
- cuándo se conectó por última vez

Si elimina un equipo guardado con la sesión iniciada, también se elimina de su cuenta.

## Qué no

- **El PIN.** Nunca se envía, ni siquiera cifrado. La aplicación para el teléfono lo guarda en el teléfono con cada equipo. El cliente de navegador no lo guarda en absoluto. Lo que un dispositivo sí conserva tras la vinculación es su propia clave y las claves de los ordenadores con los que se ha vinculado, para volver a conectarse sin el PIN. Se quedan en el dispositivo.
- En la aplicación para el teléfono, el modo elegido y si ocultó un equipo se quedan en el teléfono.
- Nada sobre su pantalla.

## En el ordenador

El host inicia sesión solo para identificarle. No tiene una lista de equipos que sincronizar, porque él mismo es el equipo: sus conexiones recientes pertenecen a ese ordenador, no a usted. Iniciar sesión en él permite que el recuento de usuarios cuente su ordenador, su teléfono y su navegador como una sola persona.

## Dónde se crean las cuentas

Screens solo puede iniciar sesión en un Universal ID que ya exista. Puede crear uno, gratis, en app.unisim.co.uk, de modo que un correo mal escrito nunca crea una cuenta por error.

---
id: email-code-sign-in
group: Privacidad y seguridad
title: ¿Por qué no hay «Iniciar sesión con Google»?
summary: Screens inicia sesión con un código enviado a su correo, el único método que funciona en todos los clientes.
---
Para iniciar sesión, escribe su dirección de correo, recibe un código de 6 dígitos y lo escribe. No hay contraseña.

## Por qué no Google

El cliente de navegador se abre desde un equipo de su propia red, con una dirección http:// sin cifrar. No puede ser de otro modo: los navegadores impiden que una página segura https:// abra el tipo de conexión local que el cliente usa para llegar al host. Google solo permite su inicio de sesión en direcciones web seguras registradas con antelación, y una dirección de la red de su casa o de su oficina nunca puede ser una de ellas. Por eso Screens usa el código por correo en todas partes: en la aplicación para el teléfono, en el navegador y en el ordenador.

## Después de iniciar sesión

Seguirá con la sesión iniciada en ese dispositivo hasta que la cierre. Si la sesión ya no puede renovarse, por ejemplo porque se eliminó la cuenta, la aplicación la cierra en ese dispositivo. La conexión a sus equipos nunca depende de ello.
