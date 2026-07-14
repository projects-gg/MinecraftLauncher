using System;
using System.IO;
using System.IO.Compression;
using System.Net.Http;
using System.Reflection;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace Projects_Launcher.Afk
{
    public class AfkDownloadProgress
    {
        public string Phase { get; set; }
        public long ReceivedBytes { get; set; }
        public long TotalBytes { get; set; }

        public int Percent
        {
            get
            {
                if (TotalBytes <= 0) return 0;
                return (int)Math.Min(100L, ReceivedBytes * 100L / TotalBytes);
            }
        }
    }

    /// <summary>
    /// AFK istemcisi (Minecraft Console Client) .NET 10 üzerinde çalışır, başlatıcı ise .NET Framework 4.8.
    /// Aynı süreçte barınamadıkları için istemci ayrı bir çalıştırılabilir olarak dağıtılır; bu sınıf onun
    /// indirilmesinden, güncellenmesinden ve yerinin bulunmasından sorumludur.
    /// </summary>
    public static class AfkRuntime
    {
        private const string VersionUrl = "https://mc.projects.gg/LauncherUpdateStream/version-afk.php";
        private const string PackageUrlFormat = "https://mc.projects.gg/LauncherUpdateStream/projects-afk-{0}.zip";

        // Paket onlarca MB olabildiği için indirmede uzun zaman aşımı gerekir; sürüm yoklaması ise
        // aynı istemciyi kullanırsa çevrimdışı makinede dakikalarca asılı kalırdı.
        private static readonly HttpClient DownloadHttp = new HttpClient { Timeout = TimeSpan.FromMinutes(10) };
        private static readonly HttpClient VersionHttp = new HttpClient { Timeout = TimeSpan.FromSeconds(10) };

        /// <summary>Kurulu istemcinin sürümü; kurulu değilse boş dizi.</summary>
        public static string InstalledVersion
        {
            get
            {
                try
                {
                    if (File.Exists(AfkPaths.VersionFile) && File.Exists(AfkPaths.ClientExe))
                        return File.ReadAllText(AfkPaths.VersionFile).Trim();
                }
                catch (Exception)
                {
                    // Sürüm dosyası okunamadıysa kurulum yokmuş gibi davranılır.
                }

                return string.Empty;
            }
        }

        /// <summary>
        /// Geliştirme sırasında derlenmiş MCC'yi başlatıcının yanındaki "afk" klasörüne koymak,
        /// her denemede sunucudan indirmeye gerek bırakmaz. Bu yol varsa indirme tamamen atlanır.
        /// </summary>
        public static string LocalDevExe
        {
            get
            {
                try
                {
                    string baseDir = Path.GetDirectoryName(Assembly.GetExecutingAssembly().Location);
                    if (string.IsNullOrEmpty(baseDir))
                        return null;

                    string candidate = Path.Combine(baseDir, "afk", AfkPaths.ClientExeName);
                    return File.Exists(candidate) ? candidate : null;
                }
                catch (Exception)
                {
                    return null;
                }
            }
        }

        /// <summary>Halihazırda çalıştırılabilir bir istemci varsa yolunu, yoksa null döndürür.</summary>
        public static string ResolveExistingExe()
        {
            string dev = LocalDevExe;
            if (dev != null)
                return dev;

            return File.Exists(AfkPaths.ClientExe) ? AfkPaths.ClientExe : null;
        }

        /// <summary>
        /// İstemcinin kurulu ve güncel olmasını sağlar, çalıştırılabilir dosyanın yolunu döndürür.
        /// Ağ erişilemiyorsa ve yerel kurulum varsa onunla devam edilir (AFK hesapları çevrimdışı bozulmasın).
        /// </summary>
        public static async Task<string> EnsureClientAsync(IProgress<AfkDownloadProgress> progress, CancellationToken cancellationToken)
        {
            string dev = LocalDevExe;
            if (dev != null)
                return dev;

            Report(progress, "Sürüm denetleniyor…", 0, 0);

            string latest = await FetchLatestVersionAsync().ConfigureAwait(false);
            string installed = InstalledVersion;

            if (string.IsNullOrEmpty(latest))
            {
                // Sürüm uç noktasına ulaşılamadı: kurulu istemci varsa onu kullan, yoksa hata ver.
                if (!string.IsNullOrEmpty(installed))
                    return AfkPaths.ClientExe;

                throw new InvalidOperationException(
                    "AFK istemcisinin sürüm bilgisi alınamadı. İnternet bağlantınızı denetleyip tekrar deneyin.");
            }

            if (installed == latest)
                return AfkPaths.ClientExe;

            string zipPath = Path.Combine(AfkPaths.Root, "projects-afk-" + latest + ".zip");
            Directory.CreateDirectory(AfkPaths.Root);

            try
            {
                await DownloadAsync(string.Format(PackageUrlFormat, latest), zipPath, progress, cancellationToken)
                    .ConfigureAwait(false);

                Report(progress, "Kuruluyor…", 0, 0);

                Directory.CreateDirectory(AfkPaths.BinDir);
                ExtractOverwrite(zipPath, AfkPaths.BinDir);

                if (!File.Exists(AfkPaths.ClientExe))
                    throw new InvalidOperationException("İndirilen AFK paketinde " + AfkPaths.ClientExeName + " bulunamadı.");

                File.WriteAllText(AfkPaths.VersionFile, latest, new UTF8Encoding(false));
            }
            finally
            {
                TryDelete(zipPath);
            }

            return AfkPaths.ClientExe;
        }

        private static async Task<string> FetchLatestVersionAsync()
        {
            try
            {
                string content = await VersionHttp.GetStringAsync(VersionUrl).ConfigureAwait(false);
                return ExtractFirstTagContent(content);
            }
            catch (Exception)
            {
                return string.Empty;
            }
        }

        /// <summary>
        /// version-*.php uçları sürümü tek bir HTML etiketinin içinde döndürür.
        /// MainWindow'daki readPhpContent ile aynı ayrıştırma, ama hatada fabric sürümüne düşmez.
        /// </summary>
        private static string ExtractFirstTagContent(string content)
        {
            if (string.IsNullOrEmpty(content))
                return string.Empty;

            bool started = false;
            StringBuilder sb = new StringBuilder();

            foreach (char character in content)
            {
                if (character == '>')
                {
                    if (!started)
                        started = true;
                }
                else if (started)
                {
                    if (character == '<')
                        break;
                    sb.Append(character);
                }
            }

            return sb.ToString().Trim();
        }

        private static async Task DownloadAsync(string url, string destination, IProgress<AfkDownloadProgress> progress,
            CancellationToken cancellationToken)
        {
            using (HttpResponseMessage response = await DownloadHttp
                       .GetAsync(url, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
                       .ConfigureAwait(false))
            {
                response.EnsureSuccessStatusCode();

                long total = response.Content.Headers.ContentLength.GetValueOrDefault();
                long received = 0;

                using (Stream source = await response.Content.ReadAsStreamAsync().ConfigureAwait(false))
                using (FileStream target = new FileStream(destination, FileMode.Create, FileAccess.Write, FileShare.None,
                           81920, true))
                {
                    byte[] buffer = new byte[81920];
                    int read;

                    while ((read = await source.ReadAsync(buffer, 0, buffer.Length, cancellationToken).ConfigureAwait(false)) > 0)
                    {
                        await target.WriteAsync(buffer, 0, read, cancellationToken).ConfigureAwait(false);
                        received += read;
                        Report(progress, "AFK istemcisi indiriliyor…", received, total);
                    }
                }
            }
        }

        /// <summary>
        /// .NET Framework'te ZipFile.ExtractToDirectory var olan dosyaların üzerine yazamaz;
        /// güncelleme yapabilmek için girdiler elle çıkarılır. Zip-Slip'e karşı hedef yol denetlenir.
        /// </summary>
        private static void ExtractOverwrite(string zipPath, string destDir)
        {
            Directory.CreateDirectory(destDir);
            string destRoot = Path.GetFullPath(destDir.TrimEnd('/', '\\')) + Path.DirectorySeparatorChar;

            using (ZipArchive archive = ZipFile.OpenRead(zipPath))
            {
                foreach (ZipArchiveEntry entry in archive.Entries)
                {
                    string destPath = Path.GetFullPath(Path.Combine(destRoot, entry.FullName));

                    if (!destPath.StartsWith(destRoot, StringComparison.OrdinalIgnoreCase))
                        continue; // arşiv hedef klasörün dışına taşmaya çalışıyor

                    if (string.IsNullOrEmpty(entry.Name))
                    {
                        Directory.CreateDirectory(destPath);
                        continue;
                    }

                    Directory.CreateDirectory(Path.GetDirectoryName(destPath));
                    entry.ExtractToFile(destPath, true);
                }
            }
        }

        private static void Report(IProgress<AfkDownloadProgress> progress, string phase, long received, long total)
        {
            if (progress == null)
                return;

            progress.Report(new AfkDownloadProgress { Phase = phase, ReceivedBytes = received, TotalBytes = total });
        }

        private static void TryDelete(string path)
        {
            try
            {
                if (File.Exists(path))
                    File.Delete(path);
            }
            catch (Exception)
            {
                // Kilitli dosya silinemeyebilir; kurulumu etkilemez.
            }
        }
    }
}
