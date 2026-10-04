Universal Screens knowledge base, Turkish. Same articles, ids and order as en.md; see en.md for the format. The app's interface is in English, so the names of its buttons and menus are kept as they appear on screen.

---
id: what-is-screens
group: Temel bilgiler
title: Universal Screens nedir?
summary: Bir telefonu ya da tarayıcıyı bilgisayarınız için sunum kumandası, dokunmatik yüzey, ayna, uzaktan kumanda ya da ek ekran olarak kullanın.
---
Universal Screens, başka bir cihazın bilgisayarınızın ekranını, faresini ve klavyesini kullanmasını sağlar. İki parçadan oluşur.

- **Ana bilgisayar (host)**, ulaşmak istediğiniz bilgisayarda çalışır: bir Mac, bir Windows PC ya da bir Linux makinesi. Bir QR kodu ve 4 haneli bir PIN gösterir, ve yalnızca penceresi açıkken bağlantı kabul eder.
- **İstemci**, bağlandığınız cihazdır: telefon uygulaması ya da başka bir cihazdaki web tarayıcısı.

## Neler yapabilirsiniz

- **Clicker:** ileri, geri ve ekranı karartma düğmeleri olan, slaytlarınızın önizlemelerini de gösteren bir sunum kumandası.
- **Trackpad:** telefonu dokunmatik yüzey olarak kullanıp imleci hareket ettirin, dokunun ve kaydırın.
- **Mirror:** bilgisayarın ekranını kontrol etmeden izleyin.
- **Remote control:** ekranı görün ve fare ile klavyeyle kontrol edin.
- **Second screen:** telefonu ek ekran olarak kullanın. Windows'ta bunun için PC'de bir sanal ekran sürücüsünün kurulu olması gerekir.

Telefon uygulaması beşini de sunar. Tarayıcı istemcisi Remote control, Mirror ve Clicker'ı sunar.

---
id: connecting
group: Temel bilgiler
title: Bilgisayarıma nasıl bağlanırım?
summary: QR kodunu tarayın, yakındaki bir bilgisayarı seçin ya da adresini ve PIN'ini yazın.
---
Önce bilgisayarda ana bilgisayar uygulamasını başlatın. Uzak kod kullanmıyorsanız (aşağıya bakın) cihazınız ve bilgisayar aynı ağda olmalıdır.

## QR kodunu tarayın

Telefon uygulamasında **Scan to connect** düğmesine dokunun ve kamerayı ana bilgisayarın penceresindeki QR koduna tutun. Kod, bilgisayarın adresini ve PIN'ini içerir, bu yüzden hiçbir şey yazmanız gerekmez. Bilgisayarın Wi-Fi ağını da içerebilir: Android 10 ve sonrasında uygulama bu ağa katılmak ister, Android de önce kendi onay istemini gösterir.

## Yakındaki bilgisayarlar

Bağlantı ekranı açıkken telefon uygulaması ağınızda bulduğu ana bilgisayarları listeler. Birine dokunun ve ana bilgisayarın kendi penceresinde gösterdiği PIN'i yazın.

## Elle yazmak

Telefon uygulamasında **Advanced** altında ana bilgisayarın adresi (örneğin 192.168.1.20:9000) ve PIN'i için alanlar vardır. Tarayıcı istemcisi bunları ana sayfasında sorar.

## Kayıtlı makineler

Bağlandığınız her makine kaydedilir, böylece bir sonraki sefer tek dokunuş yeterlidir. Telefon uygulamasında kayıtlı bir makineyi yeniden adlandırabilir, gizleyebilir ya da silebilirsiniz; **Remember next time?** ise mod seçimini atlamanızı sağlar.

## Başka bir ağdan

Bir Mac ya da Windows PC'de, ana bilgisayarın penceresindeki **Remote access (other networks)** size bir kod verir. Bu kodu ana bilgisayarın PIN'iyle birlikte tarayıcı istemcisinde **Remote (across networks)** altına girin. Bağlantı UNI·SIM'in aktarma sunucusu üzerinden geçer, bu yüzden kendi ağınızdakinden daha fazla gecikme bekleyin.

---
id: the-pin
group: Nasıl çalışır
title: PIN ne işe yarar?
summary: Bir cihazı ilk seferde ana bilgisayarla eşleştirir. Sonrasında cihaz hatırlanır.
---
Bir cihaz ilk kez bağlanırken ana bilgisayarın 4 haneli PIN'ine ihtiyaç duyar. QR kodunu taradığınızda PIN sizin için doldurulur.

PIN, cihazınızın içeri girebileceğini hiç gönderilmeden kanıtlar. Bir cihaz eşleştikten sonra cihaz ve ana bilgisayar birbirini hatırlar; cihaz, PIN değişse bile PIN olmadan yeniden bağlanır. Şifreleme hakkındaki makale bunun nasıl olduğunu açıklar.

## Her seferinde yeni bir PIN

Kendi PIN'inizi seçmediğiniz sürece ana bilgisayar her başladığında bilgisayarın güvenli rastgele sayı üretecinden yeni bir PIN alır. Bir Mac ya da Windows PC'de **Actions ▸ Regenerate PIN** hemen yeni bir PIN seçer.

## Kendi PIN'inizi seçmek

Eşleşmiş cihazlar zaten PIN olmadan yeniden bağlanır. Kendi PIN'iniz, ekrandan her seferinde yeni bir PIN okumadan yeni cihazlar eşleştirmek istediğinizde ve her seferinde güncel PIN'e ihtiyaç duyan 0.3 ve önceki sürüm uygulamalar için işe yarar. Mac ya da Windows PC'de **Actions ▸ Use my own PIN** ile, Linux'ta ise **⚙** altında belirlersiniz. Varsayılan olarak kapalıdır. 0000'a izin verilmez, çünkü bağlantıda "PIN yok" anlamına gelir. Seçtiğiniz bir PIN, siz değiştirene kadar onu öğrenmiş herkes için geçerli kalır.

## Eşleşmiş cihazlar

Mac ya da Windows PC'de **Actions ▸ Paired devices**, Linux'ta ise **⚙** altındaki aynı seçenek, bilgisayarla eşleşmiş cihazları listeler. **Forget all paired devices**, bu cihazların hepsinin PIN'i yeniden girmesini gerektirir. Eşleşmiş bir telefon ya da bilgisayar kaybolursa veya artık sizin değilse bunu yapın.

## Yanlış denemeler

Arka arkaya ilk üç yanlış PIN'in hiçbir bedeli yoktur, böylece bir yazım hatası sizi dışarıda bırakmaz. Sonrasında ana bilgisayar 1 saniye, sonra 2 saniye, her seferinde ikiye katlayarak en fazla 5 dakika bağlantı kabul etmez. Doğru PIN sayacı sıfırlar; yanlış PIN girilmeyen bir saat de aynı şeyi yapar. Bu bekleme herkes için geçerlidir: biri denemeye devam ettikçe kendi cihazlarınız da beklemek zorunda kalır.

## PIN kimde

PIN'e ya da QR kodunun bir fotoğrafına sahip olan herkes bir cihaz eşleştirip bilgisayarı kontrol edebilir ve o cihaz, siz unutturana kadar eşleşmiş kalır. Her cihaz için ayrı bir onay adımı yoktur. Ekranınızı biriyle paylaştıktan sonra yeni bir PIN oluşturun ve tanımadığınız eşleşmiş cihazları unutturun.

---
id: encryption
group: Nasıl çalışır
title: Bağlantı şifreli mi?
summary: Evet, uçtan uca. PIN'iniz hiçbir zaman gönderilmez ve bağlantının bir kaydından bulunamaz.
---
Evet. Cihazınız ana bilgisayara ulaşır ulaşmaz ikisi, şifreli bağlantılar için yayımlanmış bir tasarım olan Noise Protocol Framework'ten bir el sıkışma yürütür. Sonrasındaki her şey şifreli tünelin içinde gider: ekranın görüntüsü, tuş vuruşlarınız ve metinleriniz.

## PIN'in rolü

Bir cihaz ilk kez bağlandığında PIN'i kullanarak ana bilgisayarla eşleşir. Eşleştirme, parola ile doğrulanan bir anahtar değişimi olan SPAKE2'yi kullanır: her iki taraf da PIN'i yeni bir anahtar değişimine katar; böylece ikisi, PIN'i ya da yalnızca PIN'den hesaplanabilecek herhangi bir şeyi göndermeden aynı PIN'i bildiklerini kanıtlar. Bağlantıyı kaydeden biri, bu kaydı PIN'i tahmin etmek için kullanamaz. Canlı olarak tahmin eden birinin her bağlantıda tek bir hakkı vardır ve ana bilgisayarın yanlış PIN'lerden sonraki beklemesi bu denemeleri sınırlar.

Eşleştirme sırasında ana bilgisayar ve cihaz kalıcı anahtarlarını değiş tokuş eder ve birbirini hatırlar. Sonraki bağlantılar PIN yerine bu anahtarları kullanır; böylece cihaz PIN olmadan yeniden bağlanır ve bu anahtarlardan birine sahip olmayan biri bağlantının ortasına giremez. Her bağlantı ayrıca yeni, tek kullanımlık anahtarlar üretir; böylece bir kayıt, bir anahtar ya da PIN daha sonra öğrenilse bile okunamaz kalır.

## Aktarma sunucusu üzerinden

Uzak kodla bağlantı UNI·SIM'in aktarma sunucusundan geçer. Tarayıcı ve ana bilgisayar yine uçtan uca şifreler, bu yüzden sunucu yalnızca okuyamadığı karışık verileri iletir.

## Eski sürümler

0.3 ve önceki sürümler, yalnızca PIN'den hesaplanan bir anahtarla farklı biçimde eşleşiyordu. Bu bağlantılardan birini kaydeden biri, 10.000 PIN'in hepsini kayıt üzerinde deneyip sizinkini bulabilirdi. Güncel ana bilgisayarlar, hiçbir şey çalışmaz hâle gelmesin diye bu eski uygulamaları hâlâ içeri alır ve biri bağlandığında bir uyarı kaydeder. Onları güncelleyin.

Güncel bir telefon ya da masaüstü uygulaması asla eski yönteme geri dönmez: ana bilgisayar 0.3 ya da daha eski bir sürümse ana bilgisayarı güncellemenizi ister. Tarayıcı istemcisi eski yöntemi yalnızca ana bilgisayar başka bir şey yapamadığını bildirdiğinde kullanır, bunu oturum günlüğünde belirtir ve daha önce güncel yöntemle eşleştiği bir ana bilgisayar için bunu asla yapmaz. Şifrelemeden önceki sürümler şifresiz bağlanır ve günlük bunu da söyler.

## Teknik ayrıntı merak edenler için

Eşleştirme, PIN üzerinde SPAKE2'yi (Ed25519 grubu) ve ardından önceden paylaşılan anahtar olarak SPAKE2 sonucundan türetilen bir anahtarla Noise_XXpsk0_25519_ChaChaPoly_BLAKE2s'yi çalıştırır. Yeniden bağlanma, hatırlanan anahtarlarla Noise_XX_25519_ChaChaPoly_BLAKE2s'yi çalıştırır. 0.3 ve önceki sürümler, önceden paylaşılan anahtar olarak PIN'den türetilen bir anahtarla Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s kullanıyordu.

---
id: what-leaves-your-network
group: Gizlilik ve güvenlik
title: Ağınızın dışına ne çıkar?
summary: Ekranınız yalnızca eşlediğiniz cihaza gider. Geri kalan her şey ve nereye gittiği burada.
---
Ekranınız hiçbir zaman UNI·SIM'e yüklenmez. Kendi ağınızda, ana bilgisayardan doğrudan eşlediğiniz cihaza gider, başka hiçbir yere gitmez.

## Ağınızda kalanlar

- Ekranın görüntüsü, tuş vuruşlarınız, fare ve dokunma girişleriniz.
- Ana bilgisayarın yakındaki cihazlara yaptığı duyuru; aynı ağdaki telefonların onu listeleyebilmesi için adını ve bağlantı noktasını içerir.
- Ana bilgisayarın son bağlantılar listesi; bunu Actions menüsünden temizleyebilirsiniz.

## UNI·SIM'e gidenler ve ne zaman

- **Kullanıcı sayısı.** Screens, kaç kişinin kullandığını gösterebilmek için rastgele bir kurulum kimliği gönderir. Oturum açtıysanız hangi hesap olduğunu da bildirir, böylece cihazlarınız tek kişi sayılır. Ekranınız, PIN'iniz ya da makineleriniz hakkında hiçbir şey bunun parçası değildir.
- **Oturum açarsanız Universal ID'niz.** Oturum açma hakkındaki makale neyin saklandığını tam olarak listeler.
- **Uzaktan erişim.** Uzak kodlu bir oturum UNI·SIM'in aktarma sunucusundan geçer. Uçtan uca şifrelidir, bu yüzden sunucu onu okuyamaz.
- **Cast to a browser screen.** Telefon uygulaması bu yolla bir tarayıcı sekmesini yönettiğinde, dokunmatik yüzey ve sunum kumandası komutları aynı sunucudan geçer. Yolda korunurlar ama uçtan uca şifreli değildirler. Ekranınızın hiçbir görüntüsü gönderilmez.

İnternet bağlantısı olmayan bir ağda bunların hiçbiri gönderilmez ve Screens aynı şekilde çalışır.

## Telefonun kamerasıyla taramak

Ana bilgisayarın QR kodu bir web adresidir. Kodu telefonun kendi kamerasıyla tararsanız ve uygulama yüklü değilse, tarayıcı o sayfayı açar; ana bilgisayarın yerel adresi ve PIN'i web sitesinin aldığı adresin parçası olur. Kod bir Wi-Fi şifresi içeriyorsa, bu şifre adresin tarayıcıların hiçbir zaman göndermediği kısmında durur.

---
id: signing-in
group: Gizlilik ve güvenlik
title: Oturum açmak ne işe yarar?
summary: İsteğe bağlıdır. Oturum açtığınızda kayıtlı makineleriniz sizinle gelir, PIN'leriniz asla.
---
Bağlanmak için hiçbir hesap gerekmez. Universal ID'nizle oturum açmanın tek amacı, yeni bir telefonun ya da tarayıcının makinelerinizi zaten tanımasıdır.

## Neler eşitlenir

Telefon uygulaması ve tarayıcı istemcisi kayıtlı makinelerinizi hesabınızda saklar. Her biri için bunlar şunlardır:

- ağınızdaki adresi
- makinenin kendi adı ve işletim sistemi
- varsa ona verdiğiniz ad
- en son ne zaman bağlandığınız

Oturumunuz açıkken kayıtlı bir makineyi silmek, onu hesabınızdan da kaldırır.

## Neler eşitlenmez

- **PIN.** Hiçbir zaman gönderilmez, şifrelenmiş hâlde bile. Telefon uygulaması onu her kayıtlı makineyle birlikte telefonda tutar. Tarayıcı istemcisi ise onu hiç kaydetmez. Bir cihazın eşleştirmeden sonra sakladığı şey kendi anahtarı ve eşleştiği bilgisayarların anahtarlarıdır; böylece PIN olmadan yeniden bağlanabilir. Bunlar cihazda kalır.
- Telefon uygulamasında seçtiğiniz mod ve bir makineyi gizleyip gizlemediğiniz telefonda kalır.
- Ekranınızla ilgili hiçbir şey.

## Bilgisayarda

Ana bilgisayar yalnızca kimliğinizi belirlemek için oturum açar. Eşitlenecek bir makine listesi yoktur, çünkü makinenin kendisidir: son bağlantıları size değil, o bilgisayara aittir. Orada oturum açmak, kullanıcı sayısının bilgisayarınızı, telefonunuzu ve tarayıcınızı tek kişi saymasını sağlar.

## Hesaplar nerede oluşturulur

Screens yalnızca zaten var olan bir Universal ID ile oturum açabilir. Hesabı app.unisim.co.uk adresinde ücretsiz oluşturursunuz; böylece yanlış yazılmış bir e-posta adresi asla istenmeyen bir hesap oluşturmaz.

---
id: email-code-sign-in
group: Gizlilik ve güvenlik
title: Neden "Google ile oturum aç" yok?
summary: Screens, e-postanıza gönderilen bir kodla oturum açar; bu, her istemcide çalışan tek yöntemdir.
---
Oturum açmak için e-posta adresinizi yazarsınız, 6 haneli bir kod alırsınız ve kodu geri yazarsınız. Şifre yoktur.

## Neden Google değil

Tarayıcı istemcisi, kendi ağınızdaki bir makineden düz bir http:// adresiyle açılır. Başka türlüsü mümkün değildir: tarayıcılar, güvenli bir https:// sayfasının, istemcinin ana bilgisayara ulaşmak için kullandığı türden yerel bir bağlantı açmasını engeller. Google ise oturum açma özelliğinin yalnızca önceden kendisine kaydedilmiş güvenli web adreslerinde çalışmasına izin verir; ev ya da ofis ağınızdaki bir adres bunlardan biri olamaz. Bu yüzden Screens her yerde e-posta kodunu kullanır: telefon uygulamasında, tarayıcıda ve bilgisayarda.

## Oturum açtıktan sonra

Oturumu kapatana kadar o cihazda oturumunuz açık kalır. Oturum artık yenilenemezse, örneğin hesap silindiği için, uygulama o cihazdaki oturumunuzu kapatır. Makinelerinize bağlanmak hiçbir zaman buna bağlı değildir.
