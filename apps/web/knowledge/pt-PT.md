Universal Screens knowledge base, Portuguese (Portugal). Same articles, ids and order as en.md; see en.md for the format. The app's interface is in English, so the names of its buttons and menus are kept as they appear on screen.

---
id: what-is-screens
group: O essencial
title: O que é o Universal Screens?
summary: Um telemóvel ou um browser como comando de apresentações, touchpad, espelho, controlo remoto ou ecrã extra para o seu computador.
---
O Universal Screens permite que outro dispositivo utilize o ecrã, o rato e o teclado do seu computador. É composto por duas partes.

- **O anfitrião** corre no computador a que quer aceder: um Mac, um PC com Windows ou uma máquina com Linux. Mostra um código QR e um PIN de 4 dígitos, e só aceita ligações enquanto a janela dele estiver aberta.
- **Um cliente** é o dispositivo a partir do qual se liga: a app para telemóvel ou um browser noutro dispositivo.

## O que pode fazer com ele

- **Clicker:** um comando de apresentações com botões de seguinte, anterior e ecrã preto, e pré-visualizações dos seus diapositivos.
- **Trackpad:** utilize o telemóvel como touchpad para mover o ponteiro, tocar e deslocar.
- **Mirror:** veja o ecrã do computador, sem o controlar.
- **Remote control:** veja o ecrã e controle-o com o rato e o teclado.
- **Second screen:** utilize o telemóvel como ecrã adicional. No Windows, isto requer um controlador de ecrã virtual instalado no PC.

A app para telemóvel oferece os cinco. O cliente no browser oferece Remote control, Mirror e Clicker.

---
id: connecting
group: O essencial
title: Como me ligo ao meu computador?
summary: Leia o código QR, escolha um computador próximo ou introduza o endereço e o PIN.
---
Inicie primeiro o anfitrião no computador. O seu dispositivo e o computador têm de estar na mesma rede, a não ser que utilize um código remoto (ver abaixo).

## Ler o código QR

Na app para telemóvel, toque em **Scan to connect** e aponte a câmara para o código QR na janela do anfitrião. O código contém o endereço do computador e o PIN, por isso não há nada a escrever. Também pode conter a rede Wi-Fi do computador: no Android 10 ou posterior, a app pede então para se juntar a essa rede, e o Android mostra antes o seu próprio pedido de confirmação.

## Computadores próximos

Enquanto o ecrã de ligação estiver aberto, a app para telemóvel lista os anfitriões que encontra na sua rede. Toque num e introduza o respetivo PIN, que o anfitrião mostra na sua janela.

## Introduzir manualmente

Na app para telemóvel, **Advanced** tem campos para o endereço do anfitrião (por exemplo, 192.168.1.20:9000) e o PIN. O cliente no browser pede-os na página principal.

## Máquinas guardadas

Cada máquina a que se liga fica guardada, por isso da próxima vez basta um toque. Na app para telemóvel pode mudar o nome de uma máquina guardada, ocultá-la ou eliminá-la, e **Remember next time?** dispensa a escolha do modo.

## A partir de outra rede

**Remote access (other networks)** na janela do anfitrião (Mac, Windows ou Linux) dá-lhe um código. Introduza-o no cliente no browser em **Remote (across networks)**, juntamente com o PIN do anfitrião. A ligação passa pelo servidor de retransmissão da UNI·SIM, por isso conte com mais atraso do que na sua própria rede.

---
id: the-pin
group: Como funciona
title: Para que serve o PIN?
summary: Emparelha um dispositivo com o anfitrião da primeira vez. A partir daí, o dispositivo é reconhecido.
---
Um dispositivo precisa do PIN de 4 dígitos do anfitrião da primeira vez que se liga. Ao ler o código QR, é preenchido automaticamente.

O PIN prova que o seu dispositivo pode entrar, sem nunca ser enviado. Depois de um dispositivo ser emparelhado, o dispositivo e o anfitrião lembram-se um do outro, e volta a ligar-se sem o PIN, mesmo depois de o PIN mudar. O artigo sobre a encriptação explica como.

## Um PIN novo de cada vez

A menos que escolha o seu, o anfitrião gera um PIN novo sempre que arranca, a partir do gerador seguro de números aleatórios do computador. Num Mac ou PC com Windows, **Actions ▸ Regenerate PIN** escolhe um novo de imediato.

## Escolher o seu

Os dispositivos emparelhados voltam a ligar-se sem PIN de qualquer forma. Um PIN seu ajuda se quiser emparelhar dispositivos novos sem ler um PIN novo no ecrã, e para as apps da versão 0.3 e anteriores, que continuam a precisar do PIN atual de cada vez. Defina-o em **Actions ▸ Use my own PIN** num Mac ou PC com Windows, ou em **⚙** no Linux. Está desativado por predefinição. 0000 não é permitido, porque na ligação significa «sem PIN». Um PIN escolhido por si continua válido até o alterar, para qualquer pessoa que o conheça.

## Dispositivos emparelhados

**Actions ▸ Paired devices** num Mac ou PC com Windows, ou em **⚙** no Linux, mostra os dispositivos emparelhados com o computador. **Forget all paired devices** obriga cada um deles a introduzir de novo o PIN. Faça-o se perder um telemóvel ou computador emparelhado, ou se deixar de ser seu.

## Tentativas erradas

Os três primeiros PIN errados seguidos não têm consequências, para que um erro de escrita não o deixe de fora. A partir daí, o anfitrião deixa de aceitar ligações durante 1 segundo, depois 2, duplicando de cada vez até 5 minutos. O PIN certo repõe a contagem, tal como uma hora sem erros. A pausa aplica-se a todos: enquanto alguém continuar a tentar, os seus próprios dispositivos também têm de esperar.

## Quem o conhece

Qualquer pessoa com o PIN, ou com uma fotografia do código QR, pode emparelhar um dispositivo e controlar o computador, e esse dispositivo continua emparelhado até o esquecer. Não há uma aprovação separada para cada dispositivo. Depois de partilhar o ecrã com alguém, gere um PIN novo e esqueça os dispositivos emparelhados que não reconheça.

---
id: encryption
group: Como funciona
title: A ligação é encriptada?
summary: Sim, ponto a ponto. O seu PIN nunca é enviado, e uma gravação da ligação não serve para o descobrir.
---
Sim. Assim que o seu dispositivo chega ao anfitrião, os dois fazem um handshake do Noise Protocol Framework, um desenho publicado para ligações encriptadas. Tudo o que vem a seguir segue dentro do túnel encriptado: a imagem do ecrã, e as teclas e o texto que escreve.

## O papel do PIN

Da primeira vez que um dispositivo se liga, emparelha-se com o anfitrião através do PIN. O emparelhamento usa SPAKE2, uma troca de chaves autenticada por palavra-passe: cada lado mistura o PIN numa troca de chaves nova, pelo que os dois provam que conhecem o mesmo PIN sem o enviar, nem nada que se possa calcular apenas a partir do PIN. Quem gravar a ligação não consegue usar a gravação para adivinhar o PIN. Quem tentar em direto tem uma tentativa por ligação, e a pausa do anfitrião depois de PIN errados limita essas tentativas.

Durante o emparelhamento, o anfitrião e o dispositivo trocam chaves permanentes e lembram-se um do outro. As ligações seguintes usam essas chaves em vez do PIN, por isso o dispositivo volta a ligar-se sem ele, e quem não tiver uma dessas chaves não consegue meter-se no meio da ligação. Cada ligação cria ainda chaves novas de utilização única, por isso uma gravação continua ilegível mesmo que uma chave ou o PIN venham a ser conhecidos mais tarde.

## Através do servidor de retransmissão

Com um código remoto, a ligação passa pelo servidor de retransmissão da UNI·SIM. O browser e o anfitrião continuam a encriptar ponto a ponto, por isso o servidor só reencaminha dados baralhados que não consegue ler.

## Versões antigas

A versão 0.3 e as anteriores emparelhavam de outra forma, com uma chave calculada apenas a partir do PIN. Quem gravasse uma dessas ligações podia experimentar os 10 000 PIN na gravação e encontrar o seu. Os anfitriões atuais continuam a deixar entrar essas apps antigas, para que nada deixe de funcionar, e registam um aviso quando uma se liga. Atualize-as.

Uma app atual para telemóvel ou computador nunca volta à forma antiga: se o anfitrião for da versão 0.3 ou anterior, pede-lhe que o atualize. O cliente no browser só usa a forma antiga quando o anfitrião indica que não sabe fazer mais nada, di-lo no registo da sessão e nunca o faz com um anfitrião com que já se emparelhou da forma atual. As versões anteriores à encriptação ligam-se sem ela, e o registo também o diz.

## Para quem gosta de pormenores técnicos

O emparelhamento executa SPAKE2 (grupo Ed25519) sobre o PIN e depois Noise_XXpsk0_25519_ChaChaPoly_BLAKE2s, com uma chave derivada do resultado do SPAKE2 como chave pré-partilhada. A nova ligação executa Noise_XX_25519_ChaChaPoly_BLAKE2s com as chaves guardadas. A versão 0.3 e as anteriores usavam Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s, com uma chave derivada do PIN como chave pré-partilhada.

---
id: what-leaves-your-network
group: Privacidade e segurança
title: O que sai da sua rede?
summary: O seu ecrã vai apenas para o dispositivo emparelhado. Eis tudo o resto, e para onde vai.
---
O seu ecrã nunca é enviado para a UNI·SIM. Na sua própria rede, vai diretamente do anfitrião para o dispositivo que emparelhou, e para mais lado nenhum.

## O que fica na sua rede

- A imagem do ecrã, as teclas que prime e o que faz com o rato ou ao toque.
- O anúncio do anfitrião aos dispositivos próximos, com o nome e a porta, para que os telemóveis na mesma rede o possam listar.
- A lista de ligações recentes do anfitrião, que pode apagar no menu Actions.

## O que vai para a UNI·SIM, e quando

- **A contagem de utilizadores.** O Screens envia um ID de instalação aleatório, para poder mostrar quantas pessoas o utilizam. Com sessão iniciada, indica também a conta, para que os seus dispositivos contem como uma só pessoa. Nada sobre o seu ecrã, o seu PIN ou as suas máquinas faz parte disso.
- **O seu Universal ID, se iniciar sessão.** O artigo sobre o início de sessão indica exatamente o que é guardado.
- **Acesso remoto.** Uma sessão com código remoto passa pelo servidor de retransmissão da UNI·SIM. Está encriptada ponto a ponto, por isso o servidor não a consegue ler.
- **Cast to a browser screen.** Quando a app para telemóvel controla assim um separador do browser, os comandos de touchpad e de apresentação passam pelo mesmo servidor. Vão protegidos pelo caminho, mas não estão encriptados ponto a ponto. Não é enviada nenhuma imagem do seu ecrã.

Numa rede sem ligação à internet, nada disto é enviado, e o Screens funciona exatamente da mesma forma.

## Ler com a câmara do telemóvel

O código QR do anfitrião é um endereço web. Se o ler com a câmara do telemóvel e a app não estiver instalada, o browser abre essa página, e o endereço local do anfitrião e o PIN fazem parte do endereço que o site recebe. Uma palavra-passe de Wi-Fi, se o código tiver uma, fica na parte do endereço que os browsers nunca enviam.

---
id: signing-in
group: Privacidade e segurança
title: Para que serve iniciar sessão?
summary: É opcional. Com sessão iniciada, as suas máquinas guardadas acompanham-no, mas nunca os PIN.
---
Nada na ligação às suas máquinas exige uma conta. Iniciar sessão com o seu Universal ID serve apenas para que um telemóvel ou browser novo já conheça as suas máquinas.

## O que é sincronizado

A app para telemóvel e o cliente no browser guardam as suas máquinas na sua conta. Para cada uma, trata-se de:

- o endereço na sua rede
- o nome e o sistema operativo da própria máquina
- o nome que lhe deu, se existir
- quando se ligou pela última vez

Eliminar uma máquina guardada com sessão iniciada remove-a também da sua conta.

## O que não é

- **O PIN.** Nunca é enviado, nem sequer baralhado. A app para telemóvel guarda-o no telemóvel com cada máquina. O cliente no browser não o guarda de todo. O que um dispositivo guarda depois do emparelhamento é a sua própria chave e as chaves dos computadores com que se emparelhou, para voltar a ligar-se sem o PIN. Ficam no dispositivo.
- Na app para telemóvel, o modo escolhido e o facto de ter ocultado uma máquina ficam no telemóvel.
- Nada sobre o seu ecrã.

## No computador

O anfitrião inicia sessão apenas para o identificar. Não tem uma lista de máquinas para sincronizar, porque ele próprio é a máquina: as ligações recentes pertencem a esse computador, não a si. Iniciar sessão aí permite que a contagem de utilizadores trate o seu computador, o seu telemóvel e o seu browser como uma só pessoa.

## Onde se criam as contas

O Screens só consegue iniciar sessão num Universal ID que já exista. Cria um, gratuitamente, em app.unisim.co.uk, para que um e-mail mal escrito nunca crie uma conta por engano.

---
id: email-code-sign-in
group: Privacidade e segurança
title: Porque não existe «Iniciar sessão com o Google»?
summary: O Screens inicia sessão com um código enviado para o seu e-mail, o único método que funciona em todos os clientes.
---
Para iniciar sessão, escreve o seu endereço de e-mail, recebe um código de 6 dígitos e escreve esse código. Não há palavra-passe.

## Porque não o Google

O cliente no browser é aberto a partir de uma máquina da sua própria rede, num endereço http:// simples. Tem de ser assim: os browsers impedem que uma página segura https:// abra o tipo de ligação local que o cliente usa para chegar ao anfitrião. O Google só permite o seu início de sessão em endereços web seguros registados previamente, e um endereço da rede de sua casa ou do escritório nunca pode ser um deles. Por isso, o Screens usa o código por e-mail em todo o lado: na app para telemóvel, no browser e no computador.

## Depois de iniciar sessão

Mantém a sessão iniciada nesse dispositivo até a terminar. Se a sessão deixar de poder ser renovada, por exemplo porque a conta foi eliminada, a app termina a sessão nesse dispositivo. A ligação às suas máquinas nunca depende disso.
