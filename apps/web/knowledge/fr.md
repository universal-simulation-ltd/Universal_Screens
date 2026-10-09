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

**Remote access (other networks)** dans la fenêtre de l'hôte (Mac, Windows ou Linux) vous donne un code. Saisissez-le dans le client navigateur sous **Remote (across networks)**, avec le code PIN de l'hôte. La connexion passe par le serveur relais d'UNI·SIM : attendez-vous à plus de latence que sur votre propre réseau.

---
id: the-pin
group: Comment ça marche
title: À quoi sert le code PIN ?
summary: Il associe un appareil à l'hôte la première fois. Ensuite, l'appareil est reconnu.
---
Un appareil a besoin du code PIN à 4 chiffres de l'hôte la première fois qu'il se connecte. Scanner le QR code le remplit pour vous.

Le code PIN prouve que votre appareil a le droit d'entrer, sans jamais être envoyé. Une fois un appareil associé, l'appareil et l'hôte se souviennent l'un de l'autre, et il se reconnecte ensuite sans code PIN, même après un changement de code. L'article sur le chiffrement explique comment.

## Un nouveau code à chaque fois

Sauf si vous choisissez le vôtre, l'hôte tire un nouveau code PIN à chaque démarrage, à partir du générateur de nombres aléatoires sécurisé de l'ordinateur. Sur un Mac ou un PC Windows, **Actions ▸ Regenerate PIN** en choisit un nouveau immédiatement.

## Choisir le vôtre

Les appareils associés se reconnectent de toute façon sans code PIN. Votre propre code est utile si vous voulez associer de nouveaux appareils sans lire un nouveau code à l'écran, et pour les applications en version 0.3 ou antérieure, qui demandent toujours le code actuel à chaque fois. Définissez-le avec **Actions ▸ Use my own PIN** sur un Mac ou un PC Windows, ou sous **⚙** sous Linux. C'est désactivé par défaut. 0000 n'est pas accepté, car sur la connexion il signifie « pas de code PIN ». Un code que vous avez choisi reste valable jusqu'à ce que vous le changiez, pour quiconque l'a appris.

## Appareils associés

**Actions ▸ Paired devices** sur un Mac ou un PC Windows, ou sous **⚙** sous Linux, liste les appareils associés à l'ordinateur. **Forget all paired devices** oblige chacun d'eux à saisir de nouveau le code PIN. Faites-le si un téléphone ou un ordinateur associé est perdu, ou ne vous appartient plus.

## Mauvais codes

Les trois premiers codes erronés d'affilée ne coûtent rien, pour qu'une faute de frappe ne vous bloque pas. Ensuite, l'hôte cesse d'accepter des connexions pendant 1 seconde, puis 2, en doublant à chaque fois jusqu'à 5 minutes. Le bon code remet le compteur à zéro, tout comme une heure sans erreur. Cette pause s'applique à tout le monde : tant que quelqu'un continue d'essayer, vos propres appareils doivent attendre aussi.

## Qui le connaît

Toute personne qui a le code PIN, ou une photo du QR code, peut associer un appareil et contrôler l'ordinateur, et cet appareil reste associé jusqu'à ce que vous l'oubliiez. Il n'y a pas d'autorisation séparée pour chaque appareil. Après avoir partagé votre écran avec quelqu'un, générez un nouveau code et oubliez les appareils associés que vous ne reconnaissez pas.

---
id: encryption
group: Comment ça marche
title: La connexion est-elle chiffrée ?
summary: Oui, de bout en bout. Votre code PIN n'est jamais envoyé, et un enregistrement de la connexion ne permet pas de le retrouver.
---
Oui. Dès que votre appareil atteint l'hôte, les deux effectuent un échange initial issu du Noise Protocol Framework, une conception publiée pour les connexions chiffrées. Tout ce qui suit passe dans le tunnel chiffré : l'image de l'écran, ainsi que vos frappes et votre texte.

## Le rôle du code PIN

La première fois qu'un appareil se connecte, il s'associe à l'hôte avec le code PIN. L'association utilise SPAKE2, un échange de clés authentifié par mot de passe : chaque côté intègre le code PIN dans un nouvel échange de clés, si bien que les deux peuvent prouver qu'ils connaissent le même code sans l'envoyer, ni rien qui se calcule à partir du seul code. Quelqu'un qui enregistre la connexion ne peut pas se servir de l'enregistrement pour deviner le code PIN. Quelqu'un qui essaie en direct n'a qu'un essai par connexion, et la pause de l'hôte après des codes erronés limite ces essais.

Pendant l'association, l'hôte et l'appareil échangent des clés durables et se souviennent l'un de l'autre. Les connexions suivantes utilisent ces clés au lieu du code PIN : l'appareil se reconnecte sans lui, et quelqu'un qui n'a pas l'une de ces clés ne peut pas s'interposer dans la connexion. Chaque connexion crée aussi de nouvelles clés à usage unique : un enregistrement reste illisible même si une clé ou le code PIN est découvert plus tard.

## Par le relais

Avec un code distant, la connexion passe par le serveur relais d'UNI·SIM. Le navigateur et l'hôte chiffrent toujours de bout en bout : le relais ne fait que transmettre des données brouillées qu'il ne peut pas lire.

## Anciennes versions

Les versions 0.3 et antérieures s'associaient autrement, avec une clé calculée à partir du seul code PIN. Quelqu'un qui enregistrait une de ces connexions pouvait essayer les 10 000 codes sur l'enregistrement et trouver le vôtre. Les hôtes actuels laissent encore entrer ces anciennes applications, pour que rien ne cesse de fonctionner, et consignent un avertissement lorsqu'une d'elles se connecte. Mettez-les à jour.

Une application actuelle pour téléphone ou ordinateur ne revient jamais à l'ancienne méthode : si l'hôte est en version 0.3 ou antérieure, elle vous demande de mettre l'hôte à jour. Le client navigateur n'utilise l'ancienne méthode que lorsque l'hôte indique qu'il ne sait rien faire d'autre, le signale dans son journal de session, et ne le fait jamais avec un hôte auquel il s'est déjà associé selon la méthode actuelle. Les versions antérieures au chiffrement se connectent sans lui, et le journal le signale aussi.

## Pour les curieux de technique

L'association exécute SPAKE2 (groupe Ed25519) sur le code PIN, puis Noise_XXpsk0_25519_ChaChaPoly_BLAKE2s avec une clé dérivée du résultat de SPAKE2 comme clé pré-partagée. La reconnexion exécute Noise_XX_25519_ChaChaPoly_BLAKE2s avec les clés mémorisées. Les versions 0.3 et antérieures utilisaient Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s avec une clé dérivée du code PIN comme clé pré-partagée.

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

- **Le code PIN.** Il n'est jamais envoyé, même chiffré. L'application pour téléphone le garde sur le téléphone avec chaque machine enregistrée. Le client navigateur ne l'enregistre pas du tout. Ce qu'un appareil conserve après l'association, c'est sa propre clé et les clés des ordinateurs auxquels il est associé, pour se reconnecter sans code PIN. Elles restent sur l'appareil.
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
