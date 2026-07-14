using System;
using System.Collections.Generic;
using System.Drawing;
using System.IO;
using System.Net.Http;
using System.Threading.Tasks;

namespace Projects_Launcher.Afk
{
    /// <summary>
    /// Oyuncu kafası ikonlarını minotar.net üzerinden indirip önbellekler.
    /// Ağ hatasında ya da geçersiz isimde sessizce <see cref="Placeholder"/> döner;
    /// arayüz thread'ini bozmamak için hiçbir zaman dışarı istisna fırlatmaz.
    /// </summary>
    public static class AfkAvatar
    {
        private const string AvatarUrlFormat = "https://minotar.net/avatar/{0}/64";

        private static readonly HttpClient Http = new HttpClient { Timeout = TimeSpan.FromSeconds(15) };

        private static readonly object cacheGate = new object();

        // Yalnızca başarıyla indirilmiş sonuçlar burada tutulur.
        private static readonly Dictionary<string, Image> cache =
            new Dictionary<string, Image>(StringComparer.OrdinalIgnoreCase);

        // Devam eden indirmeler: aynı isim için eşzamanlı çağrılar aynı Task'ı paylaşsın diye.
        private static readonly Dictionary<string, Task<Image>> pending =
            new Dictionary<string, Task<Image>>(StringComparer.OrdinalIgnoreCase);

        private static readonly object placeholderGate = new object();
        private static Image placeholderImage;

        /// <summary>Ağ yokken ya da hata durumunda kullanılacak, kodla çizilmiş düz gri kafa ikonu.</summary>
        public static Image Placeholder
        {
            get
            {
                lock (placeholderGate)
                {
                    if (placeholderImage == null)
                        placeholderImage = CreatePlaceholder();
                    return placeholderImage;
                }
            }
        }

        /// <summary>
        /// Verilen kullanıcı adının 64x64 kafa ikonunu döndürür. Sonuç önbelleğe alınır; aynı isim
        /// için eşzamanlı çağrılar tek indirmeyi paylaşır. Hata ya da boş isimde Placeholder döner.
        /// </summary>
        public static Task<Image> GetHeadAsync(string nickname)
        {
            if (string.IsNullOrWhiteSpace(nickname))
                return Task.FromResult(Placeholder);

            string key = nickname.Trim();

            lock (cacheGate)
            {
                Image cached;
                if (cache.TryGetValue(key, out cached))
                    return Task.FromResult(cached);

                Task<Image> inFlight;
                if (pending.TryGetValue(key, out inFlight))
                    return inFlight;

                Task<Image> download = DownloadHeadAsync(key);
                pending[key] = download;
                return download;
            }
        }

        private static async Task<Image> DownloadHeadAsync(string nickname)
        {
            Image downloaded = null;

            try
            {
                string url = string.Format(AvatarUrlFormat, Uri.EscapeDataString(nickname));
                byte[] data = await Http.GetByteArrayAsync(url).ConfigureAwait(false);

                using (MemoryStream stream = new MemoryStream(data))
                using (Image raw = Image.FromStream(stream))
                {
                    // Akış kapanınca kaynak görüntü de geçersiz kalacağı için kalıcı bir kopya çıkarılır.
                    downloaded = new Bitmap(raw);
                }
            }
            catch (Exception)
            {
                // Ağ hatası, zaman aşımı ya da bozuk yanıt; çağıran Placeholder ile devam eder.
                downloaded = null;
            }

            lock (cacheGate)
            {
                pending.Remove(nickname);

                // Hatalı sonuç önbelleğe alınmaz: ağ sonradan düzelirse bir sonraki çağrı yeniden dener.
                if (downloaded != null)
                    cache[nickname] = downloaded;
            }

            return downloaded ?? Placeholder;
        }

        private static Image CreatePlaceholder()
        {
            Bitmap bitmap = new Bitmap(64, 64);

            using (Graphics g = Graphics.FromImage(bitmap))
            {
                g.Clear(Color.FromArgb(52, 58, 70));

                using (Brush innerBrush = new SolidBrush(Color.FromArgb(152, 162, 179)))
                {
                    const int margin = 20;
                    g.FillRectangle(innerBrush, margin, margin, 64 - margin * 2, 64 - margin * 2);
                }
            }

            return bitmap;
        }
    }
}
