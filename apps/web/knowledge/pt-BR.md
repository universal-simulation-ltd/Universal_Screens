Universal Screens knowledge base, Portuguese (Brazil). Same articles, ids and order as en.md; see en.md for the format. The app's interface is in English, so the names of its buttons and menus are kept as they appear on screen.

---
id: what-is-screens
group: O básico
title: O que é o Universal Screens?
summary: Um celular ou um navegador como passador de slides, touchpad, espelho, controle remoto ou tela extra para o seu computador.
---
O Universal Screens permite que outro dispositivo use a tela, o mouse e o teclado do seu computador. Ele tem duas partes.

- **O host** roda no computador que você quer acessar: um Mac, um PC com Windows ou uma máquina com Linux. Ele mostra um QR code e um PIN de 4 dígitos, e só aceita conexões enquanto a janela dele estiver aberta.
- **Um cliente** é de onde você se conecta: o app para celular ou um navegador em outro dispositivo.

## O que dá para fazer

- **Clicker:** um passador de slides com botões de avançar, voltar e tela preta, além de prévias dos seus slides.
- **Trackpad:** use o celular como touchpad para mover o ponteiro, tocar e rolar.
- **Mirror:** veja a tela do computador, sem controlá-la.
- **Remote control:** veja a tela e controle-a com mouse e teclado.
- **Second screen:** use o celular como tela extra. No Windows, isso exige um driver de tela virtual instalado no PC.

O app para celular oferece os cinco. O cliente no navegador oferece Remote control, Mirror e Clicker.

---
id: connecting
group: O básico
title: Como eu me conecto ao meu computador?
summary: Escaneie o QR code, escolha um computador próximo ou digite o endereço e o PIN dele.
---
Primeiro, inicie o host no computador. Seu dispositivo e o computador precisam estar na mesma rede, a não ser que você use um código remoto (veja abaixo).

## Escanear o QR code

No app para celular, toque em **Scan to connect** e aponte a câmera para o QR code na janela do host. O código traz o endereço do computador e o PIN, então não é preciso digitar nada. Ele também pode trazer a rede Wi-Fi do computador: no Android 10 ou mais recente, o app pede para entrar nessa rede, e o Android mostra antes o próprio aviso de confirmação.

## Computadores próximos

Enquanto a tela de conexão está aberta, o app para celular lista os hosts que encontra na sua rede. Toque em um e digite o PIN dele, que o host mostra na própria janela.

## Digitando

No app para celular, **Advanced** tem campos para o endereço do host (por exemplo, 192.168.1.20:9000) e o PIN. O cliente no navegador pede esses dados na página principal.

## Máquinas salvas

Cada máquina à qual você se conecta fica salva, então da próxima vez basta um toque. No app para celular, você pode renomear, ocultar ou excluir uma máquina salva, e **Remember next time?** pula a escolha do modo.

## De outra rede

**Remote access (other networks)** na janela do host (Mac, Windows ou Linux) fornece um código. Digite-o no cliente no navegador em **Remote (across networks)**, junto com o PIN do host. A conexão passa pelo servidor de retransmissão da UNI·SIM, então espere mais atraso do que na sua própria rede.

---
id: the-pin
group: Como funciona
title: Para que serve o PIN?
summary: Ele pareia um dispositivo com o host na primeira vez. Depois disso, o dispositivo é lembrado.
---
Um dispositivo precisa do PIN de 4 dígitos do host na primeira vez que se conecta. Ao escanear o QR code, ele é preenchido para você.

O PIN prova que seu dispositivo pode entrar, sem nunca ser enviado. Depois que um dispositivo é pareado, ele e o host se lembram um do outro, e ele se reconecta sem o PIN, mesmo depois que o PIN muda. O artigo sobre criptografia explica como.

## Um PIN novo a cada vez

A menos que você escolha o seu, o host gera um PIN novo sempre que é iniciado, usando o gerador seguro de números aleatórios do computador. Em um Mac ou PC com Windows, **Actions ▸ Regenerate PIN** escolhe um novo na hora.

## Escolhendo o seu

Dispositivos pareados se reconectam sem PIN de qualquer forma. Um PIN seu ajuda se você quer parear dispositivos novos sem ler um PIN novo na tela, e para apps da versão 0.3 e anteriores, que ainda precisam do PIN atual toda vez. Defina-o em **Actions ▸ Use my own PIN** em um Mac ou PC com Windows, ou em **⚙** no Linux. Isso vem desativado. 0000 não é aceito, porque na conexão significa "sem PIN". Um PIN escolhido por você continua valendo até você mudá-lo, para qualquer pessoa que o conheça.

## Dispositivos pareados

**Actions ▸ Paired devices** em um Mac ou PC com Windows, ou em **⚙** no Linux, lista os dispositivos pareados com o computador. **Forget all paired devices** faz com que todos eles precisem digitar o PIN de novo. Faça isso se um celular ou computador pareado for perdido ou não for mais seu.

## Tentativas erradas

Os três primeiros PINs errados seguidos não têm consequência, para que um erro de digitação não bloqueie você. Depois disso, o host para de aceitar conexões por 1 segundo, depois 2, dobrando a cada vez até 5 minutos. O PIN certo zera a contagem, assim como uma hora sem erros. A pausa vale para todos: enquanto alguém continuar tentando, seus próprios dispositivos também terão de esperar.

## Quem tem o PIN

Qualquer pessoa com o PIN, ou com uma foto do QR code, pode parear um dispositivo e controlar o computador, e esse dispositivo continua pareado até você esquecê-lo. Não existe uma aprovação separada para cada dispositivo. Depois de compartilhar sua tela com alguém, gere um PIN novo e esqueça os dispositivos pareados que você não reconhece.

---
id: encryption
group: Como funciona
title: A conexão é criptografada?
summary: Sim, de ponta a ponta. Seu PIN nunca é enviado, e uma gravação da conexão não serve para descobri-lo.
---
Sim. Assim que seu dispositivo chega ao host, os dois fazem um handshake do Noise Protocol Framework, um projeto publicado para conexões criptografadas. Tudo o que vem depois passa por dentro do túnel criptografado: a imagem da tela, e as teclas e o texto que você digita.

## O papel do PIN

Na primeira vez que um dispositivo se conecta, ele é pareado com o host usando o PIN. O pareamento usa SPAKE2, uma troca de chaves autenticada por senha: cada lado mistura o PIN em uma troca de chaves nova, de modo que os dois provam que conhecem o mesmo PIN sem enviá-lo, nem nada que possa ser calculado só a partir do PIN. Quem grava a conexão não consegue usar a gravação para adivinhar o PIN. Quem tenta ao vivo tem uma tentativa por conexão, e a pausa do host depois de PINs errados limita essas tentativas.

Durante o pareamento, o host e o dispositivo trocam chaves permanentes e se lembram um do outro. As conexões seguintes usam essas chaves no lugar do PIN, então o dispositivo se reconecta sem ele, e quem não tem uma dessas chaves não consegue se colocar no meio da conexão. Cada conexão também cria chaves novas de uso único, então uma gravação continua ilegível mesmo que uma chave ou o PIN sejam descobertos depois.

## Pelo servidor de retransmissão

Com um código remoto, a conexão passa pelo servidor de retransmissão da UNI·SIM. O navegador e o host continuam criptografando de ponta a ponta, então o servidor só repassa dados embaralhados que não consegue ler.

## Versões antigas

A versão 0.3 e as anteriores faziam o pareamento de outro jeito, com uma chave calculada só a partir do PIN. Quem gravasse uma dessas conexões podia testar os 10.000 PINs na gravação e encontrar o seu. Os hosts atuais ainda deixam esses apps antigos entrarem, para que nada pare de funcionar, e registram um aviso quando um deles se conecta. Atualize-os.

Um app atual para celular ou computador nunca volta ao jeito antigo: se o host for da versão 0.3 ou anterior, ele pede que você atualize o host. O cliente no navegador só usa o jeito antigo quando o host informa que não sabe fazer outra coisa, avisa isso no registro da sessão e nunca faz isso com um host com o qual já foi pareado do jeito atual. Versões anteriores à criptografia se conectam sem ela, e o registro também avisa isso.

## Para quem gosta de detalhes técnicos

O pareamento executa SPAKE2 (grupo Ed25519) sobre o PIN e depois Noise_XXpsk0_25519_ChaChaPoly_BLAKE2s, com uma chave derivada do resultado do SPAKE2 como chave pré-compartilhada. A reconexão executa Noise_XX_25519_ChaChaPoly_BLAKE2s com as chaves lembradas. A versão 0.3 e as anteriores usavam Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s, com uma chave derivada do PIN como chave pré-compartilhada.

---
id: what-leaves-your-network
group: Privacidade e segurança
title: O que sai da sua rede?
summary: Sua tela vai só para o dispositivo pareado. Aqui está todo o resto, e para onde vai.
---
Sua tela nunca é enviada para a UNI·SIM. Na sua rede, ela vai direto do host para o dispositivo que você pareou, e para mais nenhum lugar.

## O que fica na sua rede

- A imagem da tela, as teclas que você digita e o que você faz com o mouse ou pelo toque.
- O anúncio do host para os dispositivos próximos, com o nome e a porta dele, para que celulares na mesma rede possam listá-lo.
- A lista de conexões recentes do host, que você pode apagar no menu Actions dele.

## O que vai para a UNI·SIM, e quando

- **A contagem de usuários.** O Screens envia um ID de instalação aleatório para poder mostrar quantas pessoas o usam. Se você estiver conectado à sua conta, ele também informa qual é, para que seus dispositivos contem como uma só pessoa. Nada sobre sua tela, seu PIN ou suas máquinas faz parte disso.
- **Seu Universal ID, se você entrar na conta.** O artigo sobre entrar na conta lista exatamente o que é guardado.
- **Acesso remoto.** Uma sessão com código remoto passa pelo servidor de retransmissão da UNI·SIM. Ela é criptografada de ponta a ponta, então o servidor não consegue lê-la.
- **Cast to a browser screen.** Quando o app para celular controla uma aba do navegador dessa forma, os comandos de touchpad e de passador de slides passam pelo mesmo servidor. Eles vão protegidos no caminho, mas não são criptografados de ponta a ponta. Nenhuma imagem da sua tela é enviada.

Em uma rede sem conexão com a internet, nada disso é enviado, e o Screens funciona do mesmo jeito.

## Escaneando com a câmera do celular

O QR code do host é um endereço web. Se você o escanear com a câmera do celular e o app não estiver instalado, o navegador abre essa página, e o endereço local do host e o PIN fazem parte do endereço que o site recebe. Uma senha de Wi-Fi, se o código tiver uma, fica na parte do endereço que os navegadores nunca enviam.

---
id: signing-in
group: Privacidade e segurança
title: Para que serve entrar na conta?
summary: É opcional. Com a conta, suas máquinas salvas acompanham você, mas nunca os seus PINs.
---
Nada na conexão exige uma conta. Entrar com o seu Universal ID serve apenas para que um celular ou navegador novo já conheça suas máquinas.

## O que é sincronizado

O app para celular e o cliente no navegador guardam suas máquinas salvas na sua conta. Para cada uma, isso inclui:

- o endereço dela na sua rede
- o nome e o sistema operacional da própria máquina
- o nome que você deu a ela, se houver
- quando você se conectou pela última vez

Excluir uma máquina salva com a conta conectada também a remove da sua conta.

## O que não é

- **O PIN.** Ele nunca é enviado, nem mesmo embaralhado. O app para celular o guarda no celular junto com cada máquina salva. O cliente no navegador não o salva de jeito nenhum. O que um dispositivo guarda depois do pareamento é a sua própria chave e as chaves dos computadores com que foi pareado, para se reconectar sem o PIN. Elas ficam no dispositivo.
- No app para celular, o modo escolhido e se você ocultou uma máquina ficam no celular.
- Qualquer coisa sobre a sua tela.

## No computador

O host entra na conta só para identificar você. Ele não tem uma lista de máquinas para sincronizar, porque ele é a própria máquina: as conexões recentes dele pertencem àquele computador, não a você. Entrar na conta ali faz a contagem de usuários tratar seu computador, seu celular e seu navegador como uma só pessoa.

## Onde as contas são criadas

O Screens só consegue entrar em um Universal ID que já existe. Você cria um, de graça, em app.unisim.co.uk, assim um e-mail digitado errado nunca cria uma conta por engano.

---
id: email-code-sign-in
group: Privacidade e segurança
title: Por que não existe "Entrar com Google"?
summary: O Screens usa um código enviado ao seu e-mail, o único método que funciona em todos os clientes.
---
Para entrar, você digita seu e-mail, recebe um código de 6 dígitos e digita esse código. Não há senha.

## Por que não o Google

O cliente no navegador é aberto a partir de uma máquina da sua própria rede, por um endereço http:// simples. Tem de ser assim: os navegadores impedem que uma página segura https:// abra o tipo de conexão local que o cliente usa para chegar ao host. O Google só permite o login dele em endereços web seguros registrados com antecedência, e um endereço da rede da sua casa ou do escritório nunca pode ser um deles. Por isso o Screens usa o código por e-mail em todo lugar: no app para celular, no navegador e no computador.

## Depois de entrar

Você continua conectado naquele dispositivo até sair. Se o acesso não puder mais ser renovado, por exemplo porque a conta foi excluída, o app desconecta você naquele dispositivo. A conexão com suas máquinas nunca depende disso.
