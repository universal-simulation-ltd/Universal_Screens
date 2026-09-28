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
summary: Cihazlarınızı içeri alır ve aynı zamanda bağlantıyı şifreleyen anahtardır.
---
Her bağlantı, ana bilgisayarın 4 haneli PIN'ini gerektirir. QR kodunu taradığınızda PIN sizin için doldurulur.

PIN'in iki görevi vardır. Ana bilgisayar, bir cihazı içeri almadan önce PIN'i kontrol eder; PIN aynı zamanda bağlantının şifrelendiği anahtardır. Şifreleme hakkındaki makale ikinci kısmı açıklar.

## Her seferinde yeni bir PIN

Kendi PIN'inizi seçmediğiniz sürece ana bilgisayar her başladığında bilgisayarın güvenli rastgele sayı üretecinden yeni bir PIN alır. Bir Mac ya da Windows PC'de **Actions ▸ Regenerate PIN** hemen yeni bir PIN seçer.

## Kendi PIN'inizi seçmek

Kayıtlı cihazlarınızın bilgisayar yeniden başladıktan sonra tekrar bağlanmasını istiyorsanız kendi PIN'inizi belirleyebilirsiniz: Mac ya da Windows PC'de **Actions ▸ Use my own PIN**, Linux'ta ise **⚙** altında. Varsayılan olarak kapalıdır. 0000'a izin verilmez, çünkü bağlantıda "PIN yok" anlamına gelir. Seçtiğiniz bir PIN, siz değiştirene kadar onu öğrenmiş herkes için geçerli kalır.

## Yanlış denemeler

Arka arkaya ilk üç yanlış PIN'in hiçbir bedeli yoktur, böylece bir yazım hatası sizi dışarıda bırakmaz. Sonrasında ana bilgisayar 1 saniye, sonra 2 saniye, her seferinde ikiye katlayarak en fazla 5 dakika bağlantı kabul etmez. Doğru PIN sayacı sıfırlar; yanlış PIN girilmeyen bir saat de aynı şeyi yapar. Bu bekleme herkes için geçerlidir: biri denemeye devam ettikçe kendi cihazlarınız da beklemek zorunda kalır.

## PIN kimde

PIN'e ya da QR kodunun bir fotoğrafına sahip olan herkes bağlanabilir ve bilgisayarı kontrol edebilir. Her cihaz için ayrı bir onay adımı yoktur. Ekranınızı biriyle paylaştıktan sonra yeni bir PIN oluşturun.

---
id: encryption
group: Nasıl çalışır
title: Bağlantı şifreli mi?
summary: Evet, uçtan uca ve anahtar olarak PIN'inizle. Mümkün olmadığında tarayıcı istemcisi size söyler.
---
Evet. Cihazınız ana bilgisayara ulaşır ulaşmaz ikisi, şifreli bağlantılar için yayımlanmış bir tasarım olan Noise Protocol Framework'ten bir el sıkışma yürütür. Sonrasındaki her şey şifreli tünelin içinde gider: ekranın görüntüsü, tuş vuruşlarınız ve metinleriniz, hatta PIN kontrolünün kendisi.

## PIN'in rolü

El sıkışma, PIN'i ortak bir sır olarak kullanır. Yanlış PIN'e sahip bir cihaz el sıkışmayı tamamlayamaz; PIN'i bilmeden bağlantının ortasına girmeye çalışan biri de tamamlayamaz. Her bağlantı ayrıca yeni, tek kullanımlık anahtarlar üretir; böylece trafiğin bir kaydı, PIN daha sonra öğrenilse bile okunamaz kalır.

## Aktarma sunucusu üzerinden

Uzak kodla bağlantı UNI·SIM'in aktarma sunucusundan geçer. Tarayıcı ve ana bilgisayar yine uçtan uca şifreler, bu yüzden sunucu yalnızca okuyamadığı karışık verileri iletir.

## Şifreli olmadığı durumlar

Şifrelemeden önceki sürümler hâlâ şifresiz bağlanabilir. Eski bir istemci şifresiz bağlandığında ana bilgisayar bir uyarı kaydeder. Tarayıcı istemcisi ana bilgisayarın neleri desteklediğini kontrol eder ve oturum günlüğünde oturumun uçtan uca şifreli olup olmadığını belirtir. Ana bilgisayar eski bir sürümse günlük bunu söyler ve güncellemenizi ister.

## Teknik ayrıntı merak edenler için

Kullanılan desen Noise_NNpsk0_25519_ChaChaPoly_BLAKE2s'dir; önceden paylaşılan anahtar olarak PIN'den türetilen bir anahtar kullanılır.

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

- **PIN.** Bağlantının şifrelendiği anahtar olduğu için hiçbir zaman gönderilmez. Telefon uygulaması yeniden bağlanabilmek için onu her kayıtlı makineyle birlikte telefonda tutar. Tarayıcı istemcisi ise onu hiç kaydetmez.
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
