using System;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Text;
using System.Windows.Forms;

namespace Projects_Launcher.Diagnostics
{
    /// <summary>
    /// Başlatıcının ortak hata kaydı ve hata bildirimi noktası.
    ///
    /// Buradan önce her hata ayrı ayrı ele alınıyordu: premium girişin kendi "destek talebi aç"
    /// penceresi vardı, oyun başlatma hatası düz bir MessageBox'a düşüyordu, oyun süreci
    /// başladıktan sonra çökerse hiçbir iz kalmıyordu. Tek kullanıcıda görülen "Oyna'ya basınca
    /// ekran kapanıp başlatıcıya dönüyor" tarzı sorunlarda elimizde hiçbir veri olmuyordu.
    ///
    /// Artık bütün hatalar buraya uğrar: diske yazılır ve kullanıcıya kopyalanabilir tek bir
    /// pencereyle gösterilir. Böylece kullanıcı hatayı olduğu gibi destek talebine yapıştırabilir.
    /// </summary>
    public static class LauncherDiagnostics
    {
        public const string SupportUrl = "https://mc.projects.gg/support/tickets/submit/";

        private static readonly object FileGate = new object();
        private static bool _installed;

        /// <summary>Oyunun ve başlatıcının verilerini tuttuğu klasör (%AppData%\.projects).</summary>
        public static string RootDirectory
        {
            get
            {
                return Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                    ".projects");
            }
        }

        /// <summary>Başlatıcı kayıtlarının klasörü. Oyunun kendi "logs" klasörüyle karışmasın diye ayrıdır.</summary>
        public static string LogDirectory
        {
            get { return Path.Combine(RootDirectory, "launcher-logs"); }
        }

        /// <summary>
        /// Yakalanmamış istisnaları da kayda alır. Program.Main içinde, Application.Run'dan önce
        /// bir kez çağrılır: bu olmadan arayüz thread'indeki bir hata Windows'un kendi
        /// "uygulama yanıt vermiyor" penceresine düşer ve kullanıcı bize hiçbir şey iletemez.
        /// </summary>
        public static void Install()
        {
            if (_installed)
                return;

            _installed = true;

            Application.SetUnhandledExceptionMode(UnhandledExceptionMode.CatchException);

            Application.ThreadException += (sender, e) =>
                ReportException("Arayüz", e.Exception);

            AppDomain.CurrentDomain.UnhandledException += (sender, e) =>
            {
                Exception ex = e.ExceptionObject as Exception;

                if (ex != null)
                    ReportException("Arka plan görevi", ex);
                else
                    Log("hata", "Bilinmeyen tür: " + Convert.ToString(e.ExceptionObject));
            };

            Log("baslatici", "Başlatıcı açıldı. " + BuildEnvironmentInfo().Replace(Environment.NewLine, " | "));
        }

        /// <summary>Günlük kayıt dosyasına tek satır yazar. Hiçbir koşulda çağıranı düşürmez.</summary>
        public static void Log(string category, string message)
        {
            try
            {
                lock (FileGate)
                {
                    Directory.CreateDirectory(LogDirectory);

                    string file = Path.Combine(
                        LogDirectory,
                        "baslatici-" + DateTime.Now.ToString("yyyy-MM-dd", CultureInfo.InvariantCulture) + ".log");

                    File.AppendAllText(
                        file,
                        DateTime.Now.ToString("HH:mm:ss", CultureInfo.InvariantCulture) +
                        "  [" + category + "] " + message + Environment.NewLine,
                        Encoding.UTF8);
                }
            }
            catch (Exception)
            {
                // Kayıt tutulamıyorsa (disk dolu, izin yok) uygulamanın akışı bundan etkilenmemeli.
            }
        }

        /// <summary>Uzun bir metni (oyun çıktısı gibi) kendi dosyasına yazar ve yolunu döndürür.</summary>
        public static string WriteReportFile(string namePrefix, string content)
        {
            try
            {
                Directory.CreateDirectory(LogDirectory);

                string file = Path.Combine(
                    LogDirectory,
                    namePrefix + "-" + DateTime.Now.ToString("yyyyMMdd-HHmmss", CultureInfo.InvariantCulture) + ".log");

                File.WriteAllText(file, content, Encoding.UTF8);
                return file;
            }
            catch (Exception)
            {
                return null;
            }
        }

        /// <summary>Destek talebine yapıştırılacak ortam bilgisi: sürüm, işletim sistemi, mimari.</summary>
        public static string BuildEnvironmentInfo()
        {
            string version;

            try
            {
                version = Properties.Settings.Default.currentVersion;
            }
            catch (Exception)
            {
                version = "?";
            }

            if (string.IsNullOrEmpty(version))
                version = "?";

            StringBuilder sb = new StringBuilder();
            sb.Append("Başlatıcı: v").Append(version).AppendLine();
            sb.Append("Zaman: ")
              .Append(DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture))
              .AppendLine();
            sb.Append("İşletim sistemi: ").Append(Environment.OSVersion.VersionString)
              .Append(Environment.Is64BitOperatingSystem ? " (64 bit)" : " (32 bit)").AppendLine();
            sb.Append("Süreç: ").Append(Environment.Is64BitProcess ? "64 bit" : "32 bit")
              .Append(" · .NET ").Append(Environment.Version).AppendLine();

            try
            {
                sb.Append("Sürüm seçimi: ").Append(Properties.Settings.Default.SelectedVersion).AppendLine();
                sb.Append("Bellek ayarı: ").Append(Properties.Settings.Default.RamMin)
                  .Append(" - ").Append(Properties.Settings.Default.RamMax).Append(" MB").AppendLine();
            }
            catch (Exception)
            {
                // Ayarlar okunamıyorsa rapor yine de üretilmeli.
            }

            return sb.ToString();
        }

        /// <summary>
        /// Bir istisnayı kaydeder ve kullanıcıya gösterir. Arayüz dışı bir thread'den çağrılsa bile
        /// pencere doğru thread'de açılır.
        /// </summary>
        public static void ReportException(string context, Exception ex, IWin32Window owner = null)
        {
            string details =
                BuildEnvironmentInfo() +
                "Konum: " + (string.IsNullOrEmpty(context) ? "-" : context) + Environment.NewLine +
                Environment.NewLine +
                Convert.ToString(ex);

            Log("hata", (context ?? "-") + " · " + (ex == null ? "-" : ex.GetType().Name + ": " + ex.Message));

            ShowReport(
                "Başlatıcı hatası",
                "Başlatıcı bu işlemi tamamlayamadı. Uygulamayı yeniden başlatmayı deneyin; " +
                "sorun sürerse aşağıdaki bilgileri kopyalayıp destek talebi açın.",
                null,
                details,
                owner);
        }

        /// <summary>Ortak hata penceresini açar. Hangi thread'den çağrıldığına bakılmaksızın çalışır.</summary>
        public static void ShowReport(string title, string message, string hint, string details, IWin32Window owner = null)
        {
            Form host = owner as Form;

            if (host == null)
            {
                foreach (Form form in Application.OpenForms)
                {
                    if (form != null && !form.IsDisposed && form.IsHandleCreated)
                    {
                        host = form;
                        break;
                    }
                }
            }

            if (host != null && host.InvokeRequired)
            {
                try
                {
                    host.BeginInvoke((Action)(() => ShowReportCore(title, message, hint, details, host)));
                    return;
                }
                catch (Exception)
                {
                    // Pencere arada kapanmış olabilir; aşağıdaki doğrudan gösterime düşülür.
                }
            }

            ShowReportCore(title, message, hint, details, host);
        }

        private static void ShowReportCore(string title, string message, string hint, string details, Form host)
        {
            try
            {
                using (ErrorReportForm form = new ErrorReportForm(title, message, hint, details))
                {
                    if (host != null && !host.IsDisposed && host.Visible)
                        form.ShowDialog(host);
                    else
                        form.ShowDialog();
                }
            }
            catch (Exception)
            {
                // Özel pencere açılamazsa kullanıcı yine de bilgisiz kalmasın.
                MessageBox.Show(message + Environment.NewLine + Environment.NewLine + details, title);
            }
        }

        public static void OpenSupportPage()
        {
            try
            {
                Process.Start(SupportUrl);
            }
            catch (Exception)
            {
                MessageBox.Show(
                    "Destek sayfası açılamadı. Adresi tarayıcınıza yazabilirsiniz:" +
                    Environment.NewLine + SupportUrl,
                    "Destek");
            }
        }

        public static void OpenLogFolder()
        {
            try
            {
                Directory.CreateDirectory(LogDirectory);
                Process.Start("explorer.exe", "\"" + LogDirectory + "\"");
            }
            catch (Exception)
            {
                MessageBox.Show(
                    "Kayıt klasörü açılamadı. Konumu:" + Environment.NewLine + LogDirectory,
                    "Kayıtlar");
            }
        }
    }
}
