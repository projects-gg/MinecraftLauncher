; Projects Launcher kurulum sihirbazı: Türkçe metinler.
;
; tauri.conf.json > bundle > windows > nsis > customLanguageFiles ile Tauri'nin hazır
; Turkish.nsh dosyasının yerine geçer; bu yüzden Tauri'nin tüm metinleri (ilk blok) burada
; tanımlı olmalıdır. Tauri sürümü yükseltilince yeni metin eklenmiş mi kontrol edin:
; crates/tauri-bundler/src/bundle/windows/nsis/languages/Turkish.nsh
;
; İkinci blok, NSIS'in Türkçe sayfa metinlerindeki yazım hatalarını düzeltir. Aynı metni
; yeniden tanımladığı için derlemede "set multiple times" uyarısı çıkar; beklenen bir durumdur.
;
; {{product_name}} kurulum sırasında uygulama adıyla değiştirilir, silmeyin.
; Projects Studio'daki kopyası: projects-studio deposunda apps/studio/src-tauri/nsis/Turkish.nsh
; (iki dosya birlikte güncellenir)

; --- Tauri metinleri ---------------------------------------------------------------------------
LangString addOrReinstall ${LANG_TURKISH} "Bileşenleri ekle veya yeniden kur"
LangString alreadyInstalled ${LANG_TURKISH} "Zaten kurulu"
LangString alreadyInstalledLong ${LANG_TURKISH} "${PRODUCTNAME} ${VERSION} zaten kurulu. Yapmak istediğiniz işlemi seçin ve devam etmek için İleri düğmesine basın."
LangString appRunning ${LANG_TURKISH} "{{product_name}} şu anda açık. Lütfen önce uygulamayı kapatıp tekrar deneyin."
LangString appRunningOkKill ${LANG_TURKISH} "{{product_name}} şu anda açık.$\nUygulamayı kapatmak için Tamam düğmesine basın."
LangString chooseMaintenanceOption ${LANG_TURKISH} "Yapmak istediğiniz bakım işlemini seçin."
LangString choowHowToInstall ${LANG_TURKISH} "${PRODUCTNAME} uygulamasını nasıl kurmak istediğinizi seçin."
LangString createDesktop ${LANG_TURKISH} "Masaüstü kısayolu oluştur"
LangString dontUninstall ${LANG_TURKISH} "Kaldırmadan devam et"
LangString dontUninstallDowngrade ${LANG_TURKISH} "Kaldırmadan devam et (bu kurulum dosyası, kaldırmadan eski sürüme geçişe izin vermiyor)"
LangString failedToKillApp ${LANG_TURKISH} "{{product_name}} kapatılamadı. Lütfen uygulamayı kendiniz kapatıp tekrar deneyin."
LangString installingWebview2 ${LANG_TURKISH} "WebView2 kuruluyor..."
LangString newerVersionInstalled ${LANG_TURKISH} "${PRODUCTNAME} uygulamasının daha yeni bir sürümü zaten kurulu. Eski bir sürümü kurmanız önerilmez; yine de kurmak istiyorsanız önce mevcut sürümü kaldırın. Yapmak istediğiniz işlemi seçin ve devam etmek için İleri düğmesine basın."
LangString older ${LANG_TURKISH} "daha eski bir"
LangString olderOrUnknownVersionInstalled ${LANG_TURKISH} "Bilgisayarınızda ${PRODUCTNAME} uygulamasının $R4 sürümü kurulu. Kurulumdan önce mevcut sürümü kaldırmanız önerilir. Yapmak istediğiniz işlemi seçin ve devam etmek için İleri düğmesine basın."
LangString silentDowngrades ${LANG_TURKISH} "Bu kurulum dosyası eski sürüme geçişe izin vermiyor, sessiz kurulumla devam edilemez. Lütfen kurulumu normal şekilde başlatın.$\n"
LangString unableToUninstall ${LANG_TURKISH} "Kaldırma işlemi tamamlanamadı."
LangString uninstallApp ${LANG_TURKISH} "${PRODUCTNAME} uygulamasını kaldır"
LangString uninstallBeforeInstalling ${LANG_TURKISH} "Kurmadan önce mevcut sürümü kaldır"
LangString unknown ${LANG_TURKISH} "bilinmeyen bir"
LangString webview2AbortError ${LANG_TURKISH} "WebView2 kurulamadı. Uygulama bu bileşen olmadan çalışamaz. Kurulumu yeniden başlatmayı deneyin."
LangString webview2DownloadError ${LANG_TURKISH} "WebView2 kurulum dosyası indirilemedi."
LangString webview2DownloadSuccess ${LANG_TURKISH} "WebView2 kurulum dosyası indirildi"
LangString webview2Downloading ${LANG_TURKISH} "WebView2 kurulum dosyası indiriliyor..."
LangString webview2InstallError ${LANG_TURKISH} "WebView2 kurulamadı."
LangString webview2InstallSuccess ${LANG_TURKISH} "WebView2 kuruldu"
LangString deleteAppData ${LANG_TURKISH} "Uygulama önbelleğini de sil"

; --- NSIS sayfa metinleri (Modern UI 2) --------------------------------------------------------
LangString MUI_TEXT_WELCOME_INFO_TITLE ${LANG_TURKISH} "$(^NameDA) kurulum sihirbazına hoş geldiniz"
LangString MUI_TEXT_WELCOME_INFO_TEXT ${LANG_TURKISH} "Bu sihirbaz $(^NameDA) uygulamasını bilgisayarınıza kuracak.$\r$\n$\r$\nKurulum yönetici izni gerektirmez; uygulama yalnızca sizin kullanıcı hesabınıza kurulur.$\r$\n$\r$\n$_CLICK"
LangString MUI_TEXT_DIRECTORY_TITLE ${LANG_TURKISH} "Kurulum dizinini seçin"
LangString MUI_TEXT_DIRECTORY_SUBTITLE ${LANG_TURKISH} "$(^NameDA) uygulamasının kurulacağı dizini seçin."
LangString MUI_TEXT_INSTALLING_TITLE ${LANG_TURKISH} "Kuruluyor"
LangString MUI_TEXT_INSTALLING_SUBTITLE ${LANG_TURKISH} "$(^NameDA) kurulurken lütfen bekleyin."
LangString MUI_TEXT_FINISH_TITLE ${LANG_TURKISH} "Kurulum tamamlandı"
LangString MUI_TEXT_FINISH_SUBTITLE ${LANG_TURKISH} "Kurulum başarıyla tamamlandı."
LangString MUI_TEXT_ABORT_TITLE ${LANG_TURKISH} "Kurulum iptal edildi"
LangString MUI_TEXT_ABORT_SUBTITLE ${LANG_TURKISH} "Kurulum tamamlanmadı."
LangString MUI_TEXT_FINISH_INFO_TITLE ${LANG_TURKISH} "$(^NameDA) kuruldu"
LangString MUI_TEXT_FINISH_INFO_TEXT ${LANG_TURKISH} "$(^NameDA) bilgisayarınıza kuruldu.$\r$\n$\r$\nSihirbazı kapatmak için Bitir düğmesine basın."
LangString MUI_TEXT_FINISH_INFO_REBOOT ${LANG_TURKISH} "$(^NameDA) kurulumunu tamamlamak için bilgisayarınızın yeniden başlatılması gerekiyor. Şimdi yeniden başlatmak ister misiniz?"
LangString MUI_TEXT_FINISH_REBOOTNOW ${LANG_TURKISH} "Şimdi yeniden başlat"
LangString MUI_TEXT_FINISH_REBOOTLATER ${LANG_TURKISH} "Daha sonra kendim yeniden başlatacağım"
LangString MUI_TEXT_FINISH_RUN ${LANG_TURKISH} "$(^NameDA) uygulamasını başlat"
LangString MUI_UNTEXT_CONFIRM_TITLE ${LANG_TURKISH} "$(^NameDA) uygulamasını kaldır"
LangString MUI_UNTEXT_CONFIRM_SUBTITLE ${LANG_TURKISH} "Uygulama bilgisayarınızdan kaldırılacak. Oyun dosyalarınız ve dünyalarınız silinmez."
LangString MUI_UNTEXT_UNINSTALLING_TITLE ${LANG_TURKISH} "Kaldırılıyor"
LangString MUI_UNTEXT_UNINSTALLING_SUBTITLE ${LANG_TURKISH} "$(^NameDA) kaldırılırken lütfen bekleyin."
LangString MUI_UNTEXT_FINISH_TITLE ${LANG_TURKISH} "Kaldırma tamamlandı"
LangString MUI_UNTEXT_FINISH_SUBTITLE ${LANG_TURKISH} "$(^NameDA) bilgisayarınızdan kaldırıldı."
LangString MUI_UNTEXT_ABORT_TITLE ${LANG_TURKISH} "Kaldırma iptal edildi"
LangString MUI_UNTEXT_ABORT_SUBTITLE ${LANG_TURKISH} "Kaldırma işlemi tamamlanmadı."
