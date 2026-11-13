// Yayın izleme (SQOL-U1; sqol.md §1.1): yayın karosunun çubuğu ve menüsü, durumlar, "Yayın
// Bilgileri", yakınlaştırma, ekran okuyucuya söylenenler ve Ayarlar'daki "İzlerken" bölümü. Başlık,
// menü satırı, düğme ve ayar adları her kelimesi büyük; durumlar ve cümleler cümle düzeninde. Oyuncu
// adı ek almaz (adın yeri cümlenin sonu ya da "{{name}} yayını").
export const trShareView = {
  stage: {
    label: "İzlenen Yayınlar",
  },
  menu: {
    volume: "Yayın Sesi",
    mute: "Yayını Sessize Al",
    quality: "Kalite",
    duck: "Konuşulurken Yayını Kıs",
    fullscreen: "Tam Ekran",
    exitFullscreen: "Tam Ekrandan Çık",
    popout: "Ayrı Pencerede Aç",
    popin: "Launcher'a Geri Al",
    onTop: "Her Zaman Üstte",
    forward: "Bu Yayını Öne Al",
    sideBySide: "Yan Yana Göster",
    info: "Yayın Bilgileri",
  },
  quality: {
    auto: "Otomatik",
    high: "Yüksek",
    saver: "Veri Tasarrufu",
  },
  bar: {
    soundOff: "Yayın Sesi Kapalı",
    more: "Yayın Seçenekleri",
    weak: "Düşük Kalite",
    weakHint: "Bağlantı zayıf olduğu için görüntü düşük kalitede.",
  },
  state: {
    hold: "Yayın duraklatıldı.",
    min: "Paylaşılan pencere küçültüldü.",
    ended: "Yayın bitti.",
  },
  info: {
    line: "{{size}} · {{fps}} fps · {{layer}}",
    f: "Tam Kalite",
    q: "Düşük Kalite",
  },
  zoom: {
    reset: "Yakınlaştırmayı Sıfırla",
    label: "Yakınlaştırmayı Sıfırla, {{value}}",
  },
  // Ekran okuyucuya kibarca söylenenler (görünmez).
  say: {
    live: "Yayında: {{name}}",
    open: "Yayın açıldı: {{name}}",
    hold: "Yayın duraklatıldı: {{name}}",
    min: "Paylaşılan pencere küçültüldü: {{name}}",
    resume: "Yayın sürüyor: {{name}}",
    end: "Yayın bitti: {{name}}",
    soundOn: "Yayının sesi açıldı.",
    soundOff: "Yayın sessize alındı.",
    volume: "Yayın sesi {{value}}",
  },
  chat: {
    label: "Sohbet",
    unread_one: "Sohbet, {{count}} okunmamış mesaj",
    unread_other: "Sohbet, {{count}} okunmamış mesaj",
  },
  corner: {
    grow: "Büyüt: {{name}} yayını",
  },
  own: {
    label: "Yayınının Önizlemesi",
    preview: "Önizleme",
    close: "Önizlemeyi Kapat",
  },
  settings: {
    watching: "İzlerken",
    sharing: "Paylaşırken",
    quality: "Yayın Kalitesi",
    duck: "Biri Konuşunca Yayın Sesini Kıs",
    duckLevel: "Kısma Miktarı",
    duckSelf: "Ben Konuşunca da Kıs",
    thumbs: "Önizleme Resmini Paylaş",
  },
};
