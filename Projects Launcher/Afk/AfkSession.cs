using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Media;
using System.Text;
using System.Threading.Tasks;
using Newtonsoft.Json.Linq;

namespace Projects_Launcher.Afk
{
    public enum AfkState
    {
        Stopped,
        Starting,
        Connecting,
        Connected,
        Reconnecting,
        Stopping,
        Error
    }

    /// <summary>
    /// Konsola düşen tek bir günlük satırı ve üretildiği an. Zaman, satırın MCC'den geldiği
    /// anda yakalanır; böylece konsol kapanıp yeniden açılsa da eski satırlar özgün saatiyle görünür.
    /// </summary>
    public sealed class AfkLogEntry
    {
        public AfkLogEntry(DateTime time, string text)
        {
            Time = time;
            Text = text ?? string.Empty;
        }

        public DateTime Time { get; private set; }
        public string Text { get; private set; }
    }

    /// <summary>
    /// Tek bir AFK hesabının canlı oturumu: MCC alt sürecini başlatır, stdout'taki olay satırlarını
    /// ayrıştırır ve durumu arayüze bildirir.
    ///
    /// İletişim şekli MCC'nin BasicIO kipine dayanır:
    ///  - stdout: insan okuyabilir günlük satırları; "@@MCC@@" ön ekli olanlar makine olaylarıdır.
    ///  - stdin : normal etkileşimli oturumdaki gibi komut/sohbet satırları.
    ///
    /// Olaylar iş parçacığı havuzunda tetiklenir; arayüz kendi thread'ine marshal etmekle yükümlüdür.
    /// </summary>
    public class AfkSession : IDisposable
    {
        private const string EventPrefix = "@@MCC@@";
        private const int MaxLogLines = 500;
        private const int GracefulExitTimeoutMs = 4000;

        private readonly object gate = new object();
        private readonly Queue<AfkLogEntry> log = new Queue<AfkLogEntry>();

        private Process process;
        private bool stopRequested;
        private bool disposed;

        // AutoRelog süreç içinde sınırsız deneyebildiği için kopma sessizce döngüye girebilir.
        // Deneme sayısını göstererek kullanıcının yanlış sunucu adresini fark etmesini sağlıyoruz.
        private int reconnectAttempts;

        // Süreç kapanınca gösterilecek son hata sebebi: çıkış kodu her zaman güvenilir değildir.
        private string lastFailureDetail;

        // MCC'nin süreç içi AutoRelog'u yalnızca oyun-içi atılmalarda çalışır; giriş aşamasındaki
        // ret ("Bot doğrulaması başarısız") ya da anti-bot kopmalarında süreç ölür ve eskiden hiç
        // yeniden denenmezdi. Süreç beklenmedik şekilde kapandığında launcher tarafı onu artan
        // beklemeyle (15→30→45→60 sn, sonra dakikada bir sabit) sürekli yeniden başlatır. Bu alanlar
        // yeniden başlatma için gereken bağlamı tutar; sayaç oyuna girilince (joined) sıfırlanır.
        private string clientExePath;
        private string launcherNickname;
        private int restartAttempts;
        private int restartRemainingSeconds;
        private string restartReason;
        private System.Threading.Timer restartTimer;

        private const int RestartStepSeconds = 15;
        private const int RestartMaxSeconds = 60;

        public AfkSession(AfkAccount account)
        {
            if (account == null) throw new ArgumentNullException("account");
            Account = account;
            State = AfkState.Stopped;
            StatusDetail = string.Empty;
        }

        public AfkAccount Account { get; private set; }

        /// <summary>
        /// Ayar penceresi hesabı yeni bir nesneyle değiştirir (accounts[index] = clone). Oturum eski
        /// nesneyle kalırsa bir sonraki bağlanışta MCC yapılandırması eski ayarlarla yazılır; bu yüzden
        /// oturum çalışmıyorken güncel nesneye geçilir. Çalışırken dokunulmaz: kullanıcıya zaten
        /// "bağlantıyı kesip yeniden bağlanın" denir.
        /// </summary>
        public void UpdateAccount(AfkAccount account)
        {
            if (account == null || IsRunning)
                return;

            Account = account;
        }

        public AfkState State { get; private set; }

        /// <summary>Durumu açıklayan kısa Türkçe metin (hata sebebi, atılma mesajı vb.).</summary>
        public string StatusDetail { get; private set; }

        public DateTime? ConnectedAt { get; private set; }

        public int? OnlinePlayers { get; private set; }
        public double? Health { get; private set; }
        public int? Food { get; private set; }

        public bool IsRunning
        {
            get
            {
                lock (gate)
                    return process != null && !process.HasExited;
            }
        }

        public bool IsBusy
        {
            get
            {
                AfkState state = State;
                return state == AfkState.Starting || state == AfkState.Connecting ||
                       state == AfkState.Reconnecting || state == AfkState.Stopping;
            }
        }

        /// <summary>Durum ya da sayaçlar değiştiğinde tetiklenir.</summary>
        public event EventHandler Changed;

        /// <summary>MCC'nin ürettiği her insan okuyabilir günlük satırı için tetiklenir (zaman damgalı).</summary>
        public event EventHandler<AfkLogEntry> LogLine;

        public void Start(string clientExePath, string launcherNickname)
        {
            lock (gate)
            {
                if (process != null && !process.HasExited)
                    return;

                stopRequested = false;
            }

            // Yeniden başlatma alt sürecin kendisi tarafından tetiklendiğinden bağlamı saklıyoruz.
            this.clientExePath = clientExePath;
            this.launcherNickname = launcherNickname;

            CancelRestartTimer();
            reconnectAttempts = 0;
            restartAttempts = 0;
            lastFailureDetail = null;
            SetState(AfkState.Starting, "Hazırlanıyor…");

            Launch(false);
        }

        /// <summary>
        /// MCC alt sürecini fiilen başlatır. Hem ilk bağlanışta (Start) hem de beklenmedik kapanış
        /// sonrası geri çekilmeli yeniden bağlanışta (RestartTick) kullanılır.
        /// </summary>
        private void Launch(bool isRestart)
        {
            try
            {
                AfkConfigWriter.Write(Account, launcherNickname);

                ProcessStartInfo startInfo = new ProcessStartInfo
                {
                    FileName = clientExePath,
                    // İlk konum bağımsız değişkeni yapılandırma dosyası, sonuncusu konsol kipi olmak zorunda.
                    Arguments = "\"" + AfkPaths.AccountIni(Account) + "\" BasicIO-NoColor",
                    WorkingDirectory = AfkPaths.AccountDir(Account),
                    UseShellExecute = false,
                    CreateNoWindow = true,
                    RedirectStandardInput = true,
                    RedirectStandardOutput = true,
                    RedirectStandardError = true,
                    StandardOutputEncoding = Encoding.UTF8,
                    StandardErrorEncoding = Encoding.UTF8,
                };

                // MCC bu değişkeni görünce köprü botunu yükler, güncelleme denetimini atlar.
                startInfo.EnvironmentVariables["MCC_LAUNCHER_BRIDGE"] = "1";

                Process started = new Process { StartInfo = startInfo, EnableRaisingEvents = true };
                started.OutputDataReceived += OnOutputDataReceived;
                started.ErrorDataReceived += OnErrorDataReceived;
                started.Exited += OnProcessExited;

                started.Start();
                started.BeginOutputReadLine();
                started.BeginErrorReadLine();

                lock (gate)
                    process = started;

                SetState(AfkState.Connecting, "Bağlanıyor…");
            }
            catch (Exception ex)
            {
                // İlk başlatma başarısızsa (ör. yol yanlış) hata kalıcıdır. Yeniden başlatma sırasında
                // exe geçici kilitliyse geri çekilmeyle tekrar deneyip döngüyü sürdürüyoruz.
                if (isRestart && Account.AutoRelog.Enabled)
                    ScheduleRestart("Başlatılamadı: " + ex.Message);
                else
                    SetState(AfkState.Error, "Başlatılamadı: " + ex.Message);
            }
        }

        /// <summary>
        /// Oyundan düzgünce çıkmayı dener (/quit), süre dolarsa süreci sonlandırır.
        /// Arayüzü bloklamamak için bekleme arka planda yapılır.
        /// </summary>
        public void Stop()
        {
            // Geri çekilme beklerken (süreç ölü, yeniden başlatma zamanlanmış) durdurulabilmeli;
            // bu yüzden önce bekleyen yeniden başlatma iptal edilir.
            CancelRestartTimer();

            Process current;
            lock (gate)
            {
                stopRequested = true;
                current = (process != null && !process.HasExited) ? process : null;
            }

            if (current == null)
            {
                SetState(AfkState.Stopped, "Bağlı değil");
                return;
            }

            SetState(AfkState.Stopping, "Kapatılıyor…");
            SendCommand("/quit");

            Task.Run(() => WaitThenKill(current, GracefulExitTimeoutMs));
        }

        /// <summary>Başlatıcı kapanırken çağrılır: sürecin gerçekten öldüğünden emin olur.</summary>
        public void StopBlocking(int timeoutMs)
        {
            CancelRestartTimer();

            Process current;
            lock (gate)
            {
                stopRequested = true;
                if (process == null || process.HasExited)
                    return;

                current = process;
            }

            SendCommand("/quit");
            WaitThenKill(current, timeoutMs);
        }

        /// <summary>MCC'ye bir komut ya da sohbet satırı gönderir.</summary>
        public void SendCommand(string line)
        {
            if (string.IsNullOrEmpty(line))
                return;

            Process current;
            lock (gate)
            {
                current = process;
                if (current == null || current.HasExited)
                    return;
            }

            try
            {
                // ProcessStartInfo .NET Framework'te stdin kodlaması ayarlamaya izin vermez;
                // Türkçe karakterlerin bozulmaması için baytları doğrudan UTF-8 yazıyoruz.
                byte[] payload = Encoding.UTF8.GetBytes(line + "\n");
                Stream stdin = current.StandardInput.BaseStream;
                stdin.Write(payload, 0, payload.Length);
                stdin.Flush();
            }
            catch (Exception)
            {
                // Süreç komut yazılırken kapanmış olabilir; durum zaten Exited ile güncellenecek.
            }
        }

        public AfkLogEntry[] LogSnapshot()
        {
            lock (gate)
                return log.ToArray();
        }

        private void WaitThenKill(Process target, int timeoutMs)
        {
            try
            {
                if (!target.WaitForExit(timeoutMs))
                    target.Kill();
            }
            catch (Exception)
            {
                // Süreç arada kendiliğinden kapanmış olabilir.
            }
        }

        private void OnOutputDataReceived(object sender, DataReceivedEventArgs e)
        {
            if (e.Data == null)
                return;

            if (e.Data.StartsWith(EventPrefix, StringComparison.Ordinal))
            {
                HandleEvent(e.Data.Substring(EventPrefix.Length));
                return;
            }

            AppendLog(e.Data);
        }

        private void OnErrorDataReceived(object sender, DataReceivedEventArgs e)
        {
            if (e.Data == null)
                return;

            AppendLog("[hata] " + e.Data);
        }

        private void HandleEvent(string json)
        {
            JObject payload;
            try
            {
                payload = JObject.Parse(json);
            }
            catch (Exception)
            {
                return; // bozuk olay satırı oturumu düşürmemeli
            }

            string name = (string)payload["e"];
            if (string.IsNullOrEmpty(name))
                return;

            switch (name)
            {
                case "connecting":
                    SetState(AfkState.Connecting, "Sunucuya bağlanılıyor…");
                    break;

                case "joined":
                    ConnectedAt = DateTime.Now;
                    reconnectAttempts = 0;
                    // Oyuna girildi: geri çekilme sayacı sıfırlanır, sonraki kopma yine 15 sn'den başlar.
                    restartAttempts = 0;
                    CancelRestartTimer();
                    SetState(AfkState.Connected, "Oyunda");
                    break;

                case "health":
                    Health = (double?)payload["hp"];
                    Food = (int?)payload["food"];
                    RaiseChanged();
                    break;

                case "players":
                    OnlinePlayers = (int?)payload["n"];
                    RaiseChanged();
                    break;

                case "autorespond":
                    // Hesap birine otomatik yanıt gönderdi; kullanıcı isterse sesle haberdar edilir.
                    if (Account.AutoRespond != null && Account.AutoRespond.SoundEnabled)
                    {
                        try
                        {
                            SystemSounds.Asterisk.Play();
                        }
                        catch (Exception)
                        {
                            // Ses aygıtı yoksa ya da devre dışıysa oturum etkilenmemeli.
                        }
                    }
                    break;

                case "disconnected":
                    HandleDisruption(DescribeDisconnect((string)payload["reason"]), (string)payload["msg"]);
                    break;

                case "error":
                    // Giriş/ping hataları oyuna hiç girilmeden gelir; MCC sebebi metinde taşır.
                    HandleDisruption(DescribeDisconnect((string)payload["reason"]), (string)payload["msg"]);
                    break;
            }
        }

        /// <summary>
        /// Kopma ve bağlanma hatalarının ortak yolu. AutoRelog açıkken MCC süreç içinde yeniden
        /// dener, bu yüzden durumu "Hata"ya değil "Yeniden bağlanılıyor"a çekiyoruz.
        /// </summary>
        private void HandleDisruption(string reason, string message)
        {
            if (stopRequested)
                return;

            ConnectedAt = null;

            string detail = reason;
            if (!string.IsNullOrWhiteSpace(message))
                detail = string.IsNullOrEmpty(detail) ? Trim(message, 70) : detail + " — " + Trim(message, 60);

            if (string.IsNullOrWhiteSpace(detail))
                detail = "Bağlantı hatası";

            lastFailureDetail = detail;

            if (!Account.AutoRelog.Enabled)
            {
                SetState(AfkState.Error, detail);
                return;
            }

            reconnectAttempts++;
            SetState(AfkState.Reconnecting,
                "Yeniden bağlanılıyor… " + reconnectAttempts + ". deneme (" + detail + ")");
        }

        private void OnProcessExited(object sender, EventArgs e)
        {
            int exitCode;
            try
            {
                exitCode = ((Process)sender).ExitCode;
            }
            catch (Exception)
            {
                exitCode = -1;
            }

            // Süreç kapanmadan önce bildirdiği son sorun; çıkış kodu bunu her zaman yansıtmaz.
            bool hadFailure = State == AfkState.Error || State == AfkState.Reconnecting;

            ConnectedAt = null;
            OnlinePlayers = null;
            Health = null;
            Food = null;

            bool userStopped;
            lock (gate)
                userStopped = stopRequested;

            if (userStopped)
            {
                SetState(AfkState.Stopped, "Bağlı değil");
                return;
            }

            // AutoRelog kapalıysa kullanıcı otomatik dönüş istemiyor: eski (denemesiz) davranış korunur.
            if (!Account.AutoRelog.Enabled)
            {
                if (exitCode != 0)
                    SetState(AfkState.Error, DescribeExitCode(exitCode));
                else if (hadFailure)
                    SetState(AfkState.Error, string.IsNullOrEmpty(lastFailureDetail) ? "Bağlantı hatası" : lastFailureDetail);
                else
                    SetState(AfkState.Stopped, "Bağlı değil");
                return;
            }

            // AutoRelog açık: kapanış nedeni ne olursa olsun artan beklemeyle yeniden bağlan. Süreç içi
            // AutoRelog'un atladığı giriş-aşaması retleri de burada yakalanır.
            string detail = exitCode != 0
                ? DescribeExitCode(exitCode)
                : (hadFailure ? (string.IsNullOrEmpty(lastFailureDetail) ? "Bağlantı hatası" : lastFailureDetail) : "Bağlantı koptu");

            ScheduleRestart(detail);
        }

        /// <summary>
        /// Süreç beklenmedik şekilde kapandıktan sonra yeniden başlatmayı zamanlar. Bekleme süresi
        /// ardışık başarısız denemeyle artar: 15, 30, 45, 60 sn; ardından dakikada bir sabit kalır.
        /// Sayaç oyuna girilince (joined) sıfırlanır. Bekleme boyunca durum saniyede bir tazelenir.
        /// </summary>
        private void ScheduleRestart(string reason)
        {
            lock (gate)
            {
                if (disposed || stopRequested)
                    return;
            }

            restartAttempts++;
            int delay = restartAttempts * RestartStepSeconds;
            if (delay > RestartMaxSeconds)
                delay = RestartMaxSeconds;

            restartRemainingSeconds = delay;
            restartReason = reason;
            UpdateReconnectCountdown();

            System.Threading.Timer timer = new System.Threading.Timer(RestartTick, null, 1000, 1000);
            lock (gate)
            {
                if (stopRequested || disposed)
                {
                    timer.Dispose();
                    return;
                }
                if (restartTimer != null)
                    restartTimer.Dispose();
                restartTimer = timer;
            }
        }

        private void RestartTick(object state)
        {
            lock (gate)
            {
                if (disposed || stopRequested)
                    return;
            }

            if (--restartRemainingSeconds > 0)
            {
                UpdateReconnectCountdown();
                return;
            }

            CancelRestartTimer();

            // Süreç bir şekilde hâlâ yaşıyorsa yeniden başlatma; ikinci bir süreç açılmasını önler.
            lock (gate)
            {
                if (stopRequested || disposed || (process != null && !process.HasExited))
                    return;
            }

            SetState(AfkState.Reconnecting, "Yeniden bağlanılıyor… (" + restartAttempts + ". deneme)");
            Launch(true);
        }

        private void UpdateReconnectCountdown()
        {
            string suffix = string.IsNullOrEmpty(restartReason) ? string.Empty : " — " + restartReason;
            SetState(AfkState.Reconnecting,
                "Yeniden bağlanılıyor… " + restartRemainingSeconds + " sn (" + restartAttempts + ". deneme)" + suffix);
        }

        private void CancelRestartTimer()
        {
            System.Threading.Timer timer;
            lock (gate)
            {
                timer = restartTimer;
                restartTimer = null;
            }
            if (timer != null)
                timer.Dispose();
        }

        /// <summary>MCC etkileşimsiz kipte kapanış sebebini çıkış koduyla bildirir (Program.HandleFailure).</summary>
        private static string DescribeExitCode(int exitCode)
        {
            switch (exitCode)
            {
                case 1: return "Oturum kapatıldı";
                case 2: return "Sunucudan atıldınız";
                case 3: return "Bağlantı koptu";
                case 4: return "Giriş reddedildi";
                default: return "Beklenmedik kapanma (kod " + exitCode + ")";
            }
        }

        private static string DescribeDisconnect(string reason)
        {
            switch (reason)
            {
                case "InGameKick": return "Sunucudan atıldınız";
                case "LoginRejected": return "Giriş reddedildi";
                case "ConnectionLost": return "Bağlantı koptu";
                case "UserLogout": return "Oturum kapatıldı";
                // Sebep taşımayan hatalarda MCC'nin kendi iletisi daha açıklayıcıdır.
                default: return string.IsNullOrEmpty(reason) ? string.Empty : "Bağlantı kesildi";
            }
        }

        private static string Trim(string value, int maxLength)
        {
            value = value.Replace('\n', ' ').Replace('\r', ' ').Trim();
            return value.Length <= maxLength ? value : value.Substring(0, maxLength - 1) + "…";
        }

        private void SetState(AfkState state, string detail)
        {
            State = state;
            StatusDetail = detail ?? string.Empty;
            RaiseChanged();
        }

        private void RaiseChanged()
        {
            EventHandler handler = Changed;
            if (handler != null)
                handler(this, EventArgs.Empty);
        }

        private void AppendLog(string line)
        {
            AfkLogEntry entry = new AfkLogEntry(DateTime.Now, line);

            lock (gate)
            {
                log.Enqueue(entry);
                while (log.Count > MaxLogLines)
                    log.Dequeue();
            }

            EventHandler<AfkLogEntry> handler = LogLine;
            if (handler != null)
                handler(this, entry);
        }

        public void Dispose()
        {
            if (disposed)
                return;
            disposed = true;

            CancelRestartTimer();

            Process current;
            lock (gate)
            {
                current = process;
                process = null;
                stopRequested = true;
            }

            if (current == null)
                return;

            try
            {
                current.OutputDataReceived -= OnOutputDataReceived;
                current.ErrorDataReceived -= OnErrorDataReceived;
                current.Exited -= OnProcessExited;

                if (!current.HasExited)
                    current.Kill();
            }
            catch (Exception)
            {
                // Süreç zaten kapanmış olabilir.
            }
            finally
            {
                current.Dispose();
            }
        }
    }
}
