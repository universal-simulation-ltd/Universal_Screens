Universal Screens knowledge base, French. Same articles, ids and order as en.md; see en.md for the format. The app's interface is in English, so the names of its buttons and menus are kept as they appear on screen.

---
id: what-is-screens
group: Les bases
title: Qu'est-ce que Universal Screens ?
summary: Un téléphone ou un navigateur comme télécommande, pavé tactile, miroir, contrôle à distance ou écran supplémentaire pour votre ordinateur.
---
Universal Screens permet à un autre appareil d'utiliser l'écran, la souris et le clavier de votre ordinateur. L'application se compose de deux parties.

- **L'hôte** tourne sur l'ordinateur que vous voulez atteindre : un Mac, un PC Windows ou une machine Linux. Il affiche un QR code et un code PIN à 4 chiffres, et n'accepte des connexions que tant que sa fenêtre est ouverte.
- **Un client** est l'appareil depuis lequel vous vous connectez : l'application pour téléphone, ou un navigateur web sur un autre appareil.

## Ce que vous pouvez en faire

- **Clicker :** une télécommande de présentation avec les boutons suivant, précédent et écran noir, et un aperçu de vos diapositives.
- **Trackpad :** le téléphone sert de pavé tactile pour déplacer le pointeur, toucher et faire défiler.
- **Mirror :** regarder l'écran de l'ordinateur, sans le contrôler.
- **Remote control :** voir l'écran et le contrôler avec la souris et le clavier.
- **Second screen :** utiliser le téléphone comme écran supplémentaire. Sous Windows, il faut pour cela un pilote d'écran virtuel installé sur le PC.

L'application pour téléphone propose les cinq. Le client navigateur propose Remote control, Mirror et Clicker.

---
id: connecting
group: Les bases
title: Comment me connecter à mon ordinateur ?
summary: Scannez le QR code, choisissez un ordinateur à proximité, ou saisissez son adresse et son code PIN.
---
Lancez d'abord l'hôte sur l'ordinateur. Votre appareil et l'ordinateur doivent être sur le même réseau, sauf si vous utilisez un code distant (voir plus bas).

## Scanner le QR code

Dans l'application pour téléphone, touchez **Scan to connect** et pointez l'appareil photo vers le QR code affiché dans la fenêtre de l'hôte. Le code contient l'adresse de l'ordinateur et son code PIN : vous n'avez rien à saisir. Il peut aussi contenir le réseau Wi-Fi de l'ordinateur : sous Android 10 ou version ultérieure, l'application demande alors à rejoindre ce réseau, et Android affiche d'abord sa propre demande de confirmation.

## Ordinateurs à proximité

Tant que l'écran de connexion est ouvert, l'application pour téléphone affiche les hôtes qu'elle trouve sur votre réseau. Touchez-en un et saisissez son code PIN, que l'hôte affiche dans sa fenêtre.

## Saisie manuelle

Dans l'application pour téléphone, **Advanced** contient des champs pour l'adresse de l'hôte (par exemple 192.168.1.20:9000) et son code PIN. Le client navigateur les demande sur sa page principale.

## Machines enregistrées

Chaque machine à laquelle vous vous connectez est enregistrée : la fois suivante, un seul geste suffit. Dans l'application pour téléphone, vous pouvez renommer, masquer ou supprimer une machine enregistrée, et **Remember next time?** vous évite de choisir le mode.

## Depuis un autre réseau

Sur un Mac ou un PC Windows, **Remote access (other networks)** dans la fenêtre de l'hôte vous donne un code. Saisissez-le dans le client navigateur sous **Remote (across networks)**, avec le code PIN de l'hôte. La connexion passe par le serveur relais d'UNI·SIM : attendez-vous à plus de latence que sur votre propre réseau.

---
id: the-pin
group: Comment ça marche
title: À quoi sert le code PIN ?
summary: Il laisse entrer vos appareils, et c'est aussi la clé qui chiffre la connexion.
---
Chaque connexion demande le code PIN à 4 chiffres de l'hôte. Scanner le QR code le remplit pour vous.

Le code PIN a deux rôles. L'hôte le vérifie avant de laisser entrer un appareil, et c'est aussi la clé avec laquelle la connexion est chiffrée. L'article sur le chiffrement explique ce second rôle.

## Un nouveau code à chaque fois

Sauf si vous choisissez le vôtre, l'hôte tire un nouveau code PIN à chaque démarrage, à partir du générateur de nombres aléatoires sécurisé de l'ordinateur. Sur un Mac ou un PC Windows, **Actions ▸ Regenerate PIN** en choisit un nouveau immédiatement.

## Choisir le vôtre

Si vous voulez que vos appareils enregistrés se reconnectent après un redémarrage de l'ordinateur, vous pouvez définir votre propre code PIN : **Actions ▸ Use my own PIN** sur un Mac ou un PC Windows, ou sous **⚙** sous Linux. C'est désactivé par défaut. 0000 n'est pas accepté, car sur la connexion il signifie « pas de code PIN ». Un code que vous avez choisi reste valable jusqu'à ce que vous le changiez, pour quiconque l'a appris.

## Mauvais codes

Les trois premiers codes erronés d'affilée ne coûtent rien, pour qu'une faute de frappe ne vous bloque pas. Ensuite, l'hôte cesse d'accepter des connexions pendant 1 seconde, puis 2, en doublant à chaque fois jusqu'à 5 minutes. Le bon code remet le compteur à zéro, tout comme une heure sans erreur. Cette pause s'applique à tout le monde : tant que quelqu'un continue d'essayer, vos propres appareils doivent attendre aussi.

## Qui le connaît

Toute personne qui a le code PIN, ou une photo du QR code, peut se connecter et contrôler l'ordinateur. Il n'y a pas d'autorisation séparée pour chaque appareil. Après avoir partagé votre écran avec quelqu'un, générez un nouveau code.

---
id: encryption
group: Comment ça marche
title: La connexion est-elle chiffrée ?
summary: Oui, de bout en bout, avec votre code PIN comme clé. Le client navigateur vous prévient si ce n'est pas possible.
---
Oui. Dès que votre appareil atteint l'hôte, les deux effectuent un échange initial issu du Noise Protocol Framework, une conception publiée pour les connexions chiffrées. Tout ce qui suit passe dans le tunnel chiffré : l'image de l'écran, vos frappes et votre texte, et la vérification du code PIN elle-même.

## Le rôle du code PIN

L'échange initial intègre le code PIN comme secret partagé. Un appareil qui a le mauvais code ne peut pas le mener à bien, pas plus que quelqu'un qui tenterait de s'interposer dans la connexion sans le code. Chaque connexion crée aussi de nouvelles clés à usage unique : un enregistrement du trafic reste illisible même si le code PIN est découvert plus tard.

## Par le relais

Avec un code distant, la connexion passe par le serveur relais d'UNI·SIM. Le navigateur et l'hôte chiffrent toujours de bout en bout : le relais ne fait que transmettre des données brouillées qu'il ne peut pas lire.

## Quand elle n'est pas chiffrée

Les versions antérieures au chiffrement se connectent encore sans lui. L'hôte consigne un avertissement lorsqu'un ancien client se connecte en clair. Le client navigateur vérifie ce dont l'hôte est capable et indique dans son journal de session si la session est chiffrée de bout en bout. Si l'hôte est une ancienne version, le journal le signale et vous invite à le mettre à jour.

## Pour les curieux de technique

Le modèle est Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s, avec une clé dérivée du code PIN comme clé pré-partagée.

---
id: what-leaves-your-network
group: Confidentialité et sécurité
title: Qu'est-ce qui quitte votre réseau ?
summary: Votre écran ne va qu'à l'appareil que vous avez associé. Voici tout le reste, et sa destination.
---
Votre écran n'est jamais envoyé à UNI·SIM. Sur votre propre réseau, il va directement de l'hôte à l'appareil que vous avez associé, et nulle part ailleurs.

## Ce qui reste sur votre réseau

- L'image de l'écran, vos frappes au clavier, et ce que vous faites avec la souris ou au toucher.
- L'annonce de l'hôte aux appareils à proximité, qui contient son nom et son port pour que les téléphones du même réseau puissent l'afficher.
- La liste des connexions récentes de l'hôte, que vous pouvez effacer depuis son menu Actions.

## Ce qui va à UNI·SIM, et quand

- **Le nombre d'utilisateurs.** Screens envoie un identifiant d'installation aléatoire, pour pouvoir afficher combien de personnes l'utilisent. Si vous êtes connecté, il indique aussi le compte, pour que vos appareils comptent comme une seule personne. Rien sur votre écran, votre code PIN ou vos machines n'en fait partie.
- **Votre Universal ID, si vous vous connectez.** L'article sur la connexion au compte détaille exactement ce qui est conservé.
- **L'accès à distance.** Une session avec un code distant passe par le relais d'UNI·SIM. Elle est chiffrée de bout en bout : le relais ne peut pas la lire.
- **Cast to a browser screen.** Lorsque l'application pour téléphone pilote ainsi un onglet de navigateur, ses commandes de pavé tactile et de télécommande passent par le même relais. Elles sont protégées en chemin, mais elles ne sont pas chiffrées de bout en bout. Aucune image de votre écran n'est envoyée.

Sur un réseau sans connexion Internet, rien de tout cela n'est envoyé, et Screens fonctionne exactement de la même façon.

## Scanner avec l'appareil photo du téléphone

Le QR code de l'hôte est une adresse web. Si vous le scannez avec l'appareil photo du téléphone et que l'application n'est pas installée, le navigateur ouvre cette page, et l'adresse locale de l'hôte ainsi que son code PIN font partie de l'adresse que reçoit le site. Un mot de passe Wi-Fi, si le code en contient un, se trouve dans la partie de l'adresse que les navigateurs n'envoient jamais.

---
id: signing-in
group: Confidentialité et sécurité
title: À quoi sert la connexion au compte ?
summary: Elle est facultative. Une fois connecté, vos machines enregistrées vous suivent, mais jamais vos codes PIN.
---
Rien dans la connexion à vos machines ne demande de compte. Vous connecter avec votre Universal ID sert uniquement à ce qu'un nouveau téléphone ou navigateur connaisse déjà vos machines.

## Ce qui est synchronisé

L'application pour téléphone et le client navigateur conservent vos machines enregistrées avec votre compte. Pour chacune, il s'agit de :

- son adresse sur votre réseau
- le nom et le système d'exploitation de la machine
- le nom que vous lui avez donné, le cas échéant
- la date de votre dernière connexion

Supprimer une machine enregistrée lorsque vous êtes connecté la retire aussi de votre compte.

## Ce qui ne l'est pas

- **Le code PIN.** C'est la clé qui chiffre la connexion, il n'est donc jamais envoyé. L'application pour téléphone le garde sur le téléphone avec chaque machine enregistrée, pour pouvoir se reconnecter. Le client navigateur ne l'enregistre pas du tout.
- Dans l'application pour téléphone, le mode choisi et le fait d'avoir masqué une machine restent sur le téléphone.
- Tout ce qui concerne votre écran.

## Sur l'ordinateur

L'hôte se connecte uniquement pour vous identifier. Il n'a pas de liste de machines à synchroniser, car il est lui-même la machine : ses connexions récentes appartiennent à cet ordinateur, pas à vous. Vous y connecter permet au nombre d'utilisateurs de compter votre ordinateur, votre téléphone et votre navigateur comme une seule personne.

## Où les comptes sont créés

Screens ne peut se connecter qu'à un Universal ID qui existe déjà. Vous en créez un, gratuitement, sur app.unisim.co.uk : une adresse e-mail mal saisie ne peut donc jamais créer un compte par erreur.

---
id: email-code-sign-in
group: Confidentialité et sécurité
title: Pourquoi n'y a-t-il pas de « Se connecter avec Google » ?
summary: Screens vous connecte avec un code envoyé par e-mail, la seule méthode qui fonctionne dans tous les clients.
---
Pour vous connecter, vous saisissez votre adresse e-mail, vous recevez un code à 6 chiffres, puis vous saisissez ce code. Il n'y a pas de mot de passe.

## Pourquoi pas Google

Le client navigateur s'ouvre depuis une machine de votre propre réseau, à une simple adresse http://. Il ne peut pas en être autrement : les navigateurs empêchent une page sécurisée https:// d'ouvrir le type de connexion locale que le client utilise pour atteindre l'hôte. Or Google n'autorise sa connexion que sur des adresses web sécurisées enregistrées auprès de lui à l'avance, et une adresse de votre réseau domestique ou professionnel ne pourra jamais en faire partie. Screens utilise donc le code par e-mail partout : dans l'application pour téléphone, dans le navigateur et sur l'ordinateur.

## Une fois connecté

Vous restez connecté sur cet appareil jusqu'à ce que vous vous déconnectiez. Si la connexion ne peut plus être renouvelée, par exemple parce que le compte a été supprimé, l'application vous déconnecte sur cet appareil. La connexion à vos machines n'en dépend jamais.
