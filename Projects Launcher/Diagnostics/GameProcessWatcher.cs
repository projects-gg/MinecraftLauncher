using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace Projects_Launcher.Diagnostics
{
    /// <summary>
    /// Oyun sürecini başlatır ve çıktısını yakalar.
    ///
    /// Önceden oyun <see cref="Process.Start()"/> ile başlatılıp bırakılıyordu: çıktısı hiçbir yere
    /// yazılmıyor, çıkış kodu okunmuyordu. Oyun ilk saniyede kapandığında başlatıcı bunu normal bir
    /// çıkıştan ayırt edemiyor ve sadece geri geliyordu ("Oyna'ya basınca ekran kapanıp başlatıcıya
    /// dönüyor"). Artık standart çıktı/hata akışları okunur, diske yazılır, çöküş ayırt edilir ve
    /// oyunun kendi crash-report dosyası rapora eklenir.
    /// </summary>
    internal sealed class GameProcessWatcher
    {
        /// <summary>Bu süreden kısa süren çıkışlar, çıkış kodu 0 olsa bile şüpheli sayılır.</summary>
        private const int EarlyExitSeconds = 20;

        private const int MaxBufferedLines = 3000;
        private const long MaxLogBytes = 8L * 1024 * 1024;

        private readonly Process _process;
        private readonly string _gameDirectory;
        private readonly string _versionName;
        private readonly object _gate = new object();
        private readonly Queue<string> _tail = new Queue<string>();
        private readonly TaskCompletionSource<bool> _exited = new TaskCompletionSource<bool>();

        private StreamWriter _writer;
        private string _outputLogPath;
        private long _writtenBytes;
        private DateTime _startedAt;
        private DateTime _crashScanFrom;

        internal GameProcessWatcher(Process process, string gameDirectory, string versionName)
        {
            _process = process;
            _gameDirectory = gameDirectory;
            _versionName = versionName;
        }

        /// <summary>Oyun çöktüğünde tetiklenir. Arayüz thread'inde çalışmaz; çağıran marshal etmelidir.</summary>
        internal event EventHandler<GameCrashEventArgs> Crashed;

        /// <summary>Süreç sona erdiğinde (çökme olsun olmasın) tetiklenir.</summary>
        internal event EventHandler Exited;

        internal bool Failed { get; private set; }

        /// <summary>
        /// Süreci başlatır. Yönlendirme ayarları başlatmadan önce, okuma ise hemen sonra
        /// yapılmalıdır: aksi halde boru dolduğunda oyun kilitlenir.
        /// </summary>
        internal void Start()
        {
            _startedAt = DateTime.Now;
            _crashScanFrom = _startedAt.AddSeconds(-5);

            bool captureEnabled = TryEnableCapture();

            _process.EnableRaisingEvents = true;
            _process.Exited += OnProcessExited;

            _process.Start();

            if (captureEnabled)
            {
                _process.BeginOutputReadLine();
                _process.BeginErrorReadLine();
            }

            LauncherDiagnostics.Log(
                "oyun",
                "Başlatıldı. Sürüm: " + (_versionName ?? "?") +
                " · çıktı yakalama: " + (captureEnabled ? "açık" : "kapalı") +
                (_outputLogPath == null ? string.Empty : " · kayıt: " + Path.GetFileName(_outputLogPath)));
        }

        /// <summary>Süreç verilen süre içinde kapandıysa true döner.</summary>
        internal async Task<bool> WaitForExitAsync(int millisecondsTimeout)
        {
            Task completed = await Task.WhenAny(_exited.Task, Task.Delay(millisecondsTimeout));
            return completed == _exited.Task;
        }

        private bool TryEnableCapture()
        {
            try
            {
                // Yönlendirme yalnızca kabuk kullanılmadığında mümkündür; CmlLib zaten böyle kurar.
                _process.StartInfo.UseShellExecute = false;
                _process.StartInfo.CreateNoWindow = true;
                _process.StartInfo.RedirectStandardOutput = true;
                _process.StartInfo.RedirectStandardError = true;
                _process.StartInfo.StandardOutputEncoding = Encoding.UTF8;
                _process.StartInfo.StandardErrorEncoding = Encoding.UTF8;

                _process.OutputDataReceived += (s, e) => Append(e.Data);
                _process.ErrorDataReceived += (s, e) => Append(e.Data);

                Directory.CreateDirectory(LauncherDiagnostics.LogDirectory);

                _outputLogPath = Path.Combine(
                    LauncherDiagnostics.LogDirectory,
                    "oyun-" + DateTime.Now.ToString("yyyyMMdd-HHmmss", CultureInfo.InvariantCulture) + ".log");

                _writer = new StreamWriter(_outputLogPath, false, Encoding.UTF8) { AutoFlush = true };
                return true;
            }
            catch (Exception ex)
            {
                // Yakalama kurulamazsa oyun yine de açılmalı; sadece teşhis verisi olmaz.
                LauncherDiagnostics.Log("oyun", "Çıktı yakalama kurulamadı: " + ex.Message);
                _writer = null;
                _outputLogPath = null;
                return false;
            }
        }

        private void Append(string line)
        {
            if (line == null)
                return;

            lock (_gate)
            {
                _tail.Enqueue(line);
                if (_tail.Count > MaxBufferedLines)
                    _tail.Dequeue();

                if (_writer == null || _writtenBytes > MaxLogBytes)
                    return;

                try
                {
                    _writer.WriteLine(line);
                    _writtenBytes += line.Length + 2;
                }
                catch (Exception)
                {
                    _writer = null;
                }
            }
        }

        private void OnProcessExited(object sender, EventArgs e)
        {
            int exitCode;

            try
            {
                exitCode = _process.ExitCode;
            }
            catch (Exception)
            {
                exitCode = -1;
            }

            TimeSpan runtime = DateTime.Now - _startedAt;

            lock (_gate)
            {
                if (_writer != null)
                {
                    try
                    {
                        _writer.Flush();
                        _writer.Dispose();
                    }
                    catch (Exception)
                    {
                        // Dosya zaten kapanmış olabilir.
                    }

                    _writer = null;
                }
            }

            string crashReport = FindCrashReport();
            bool earlyExit = runtime.TotalSeconds < EarlyExitSeconds;

            Failed = exitCode != 0 || (earlyExit && crashReport != null);

            LauncherDiagnostics.Log(
                "oyun",
                "Kapandı. Çıkış kodu: " + exitCode +
                " · süre: " + ((int)runtime.TotalSeconds) + " sn" +
                (crashReport == null ? string.Empty : " · crash-report: " + Path.GetFileName(crashReport)));

            _exited.TrySetResult(true);

            EventHandler exitedHandler = Exited;
            if (exitedHandler != null)
                exitedHandler(this, EventArgs.Empty);

            if (!Failed)
                return;

            EventHandler<GameCrashEventArgs> crashedHandler = Crashed;
            if (crashedHandler == null)
                return;

            string tail = TailSnapshot();

            crashedHandler(this, new GameCrashEventArgs(
                BuildHint(tail, crashReport, exitCode),
                BuildDetails(exitCode, runtime, tail, crashReport),
                _outputLogPath));
        }

        private string TailSnapshot()
        {
            lock (_gate)
                return string.Join(Environment.NewLine, _tail.ToArray());
        }

        /// <summary>Oyunun bu çalıştırmada ürettiği crash-report dosyasını bulur.</summary>
        private string FindCrashReport()
        {
            try
            {
                string folder = Path.Combine(_gameDirectory, "crash-reports");

                if (!Directory.Exists(folder))
                    return null;

                return new DirectoryInfo(folder)
                    .GetFiles("crash-*.txt")
                    .Where(f => f.LastWriteTime >= _crashScanFrom)
                    .OrderByDescending(f => f.LastWriteTime)
                    .Select(f => f.FullName)
                    .FirstOrDefault();
            }
            catch (Exception)
            {
                return null;
            }
        }

        /// <summary>
        /// Bilinen çökme imzalarını kullanıcının anlayacağı bir cümleye çevirir. Eşleşme yoksa
        /// null döner ve pencerede yalnızca genel açıklama görünür.
        /// </summary>
        private static string BuildHint(string output, string crashReportPath, int exitCode)
        {
            string haystack = output ?? string.Empty;

            try
            {
                if (crashReportPath != null)
                    haystack += Environment.NewLine + File.ReadAllText(crashReportPath);
            }
            catch (Exception)
            {
                // Crash-report okunamazsa yalnızca konsol çıktısına bakılır.
            }

            if (Contains(haystack, "Could not reserve enough space for object heap") ||
                Contains(haystack, "Error occurred during initialization of VM") ||
                Contains(haystack, "Could not create the Java Virtual Machine"))
            {
                return "Ayarlarda seçilen bellek miktarı cihazınız için fazla görünüyor. " +
                       "Ayarlar bölümünden en yüksek bellek değerini düşürüp yeniden deneyin.";
            }

            if (Contains(haystack, "OutOfMemoryError"))
            {
                return "Oyun bellek yetersizliğinden kapandı. Ayarlar bölümünden ayrılan belleği " +
                       "artırın ya da arka planda açık uygulamaları kapatın.";
            }

            if (Contains(haystack, "UnsupportedClassVersionError") ||
                Contains(haystack, "has been compiled by a more recent version of the Java"))
            {
                return "Kurulu Java sürümü bu Minecraft sürümü için uygun değil. " +
                       "Başlatıcıyı kapatıp yeniden açın; Java otomatik olarak yeniden kurulacaktır.";
            }

            if (Contains(haystack, "does not support required extensions") ||
                Contains(haystack, "does not have required feature") ||
                Contains(haystack, "Can't getDevice() before it was initialized") ||
                Contains(haystack, "Failed to create window") ||
                Contains(haystack, "Pixel format not accelerated") ||
                Contains(haystack, "GLFW error") ||
                Contains(haystack, "EXCEPTION_ACCESS_VIOLATION") ||
                exitCode == -1073741819)
            {
                return "Oyun ekran kartınızı hazırlayamadan kapandı. Bu genellikle eski bir ekran " +
                       "kartı sürücüsünden kaynaklanır: sürücüyü güncelleyip yeniden deneyin. " +
                       "Sorun sürerse modsuz (vanilla) bir sürümle deneyin.";
            }

            if (Contains(haystack, "Mixin apply failed") ||
                Contains(haystack, "MixinApplyError") ||
                Contains(haystack, "Incompatible mod set") ||
                Contains(haystack, "net.fabricmc.loader.impl.FormattedException"))
            {
                return "Kurulu modlardan biri bu sürümle uyuşmuyor. Modsuz (vanilla) bir sürüm " +
                       "seçip deneyin; oyun açılıyorsa sorun mod tarafındadır.";
            }

            if (Contains(haystack, "Access is denied") || Contains(haystack, "UnauthorizedAccessException"))
            {
                return "Oyun dosyalarına erişilemedi. Güvenlik yazılımınız başlatıcıyı engelliyor olabilir.";
            }

            if (string.IsNullOrWhiteSpace(output))
            {
                return "Oyun hiçbir çıktı üretmeden kapandı. Güvenlik yazılımınız Java'yı " +
                       "engelliyor olabilir; başlatıcıyı istisnalara ekleyip yeniden deneyin.";
            }

            return null;
        }

        private static bool Contains(string haystack, string needle)
        {
            return haystack.IndexOf(needle, StringComparison.OrdinalIgnoreCase) >= 0;
        }

        private string BuildDetails(int exitCode, TimeSpan runtime, string tail, string crashReportPath)
        {
            StringBuilder sb = new StringBuilder();

            sb.Append(LauncherDiagnostics.BuildEnvironmentInfo());
            sb.Append("Oyun sürümü: ").Append(_versionName ?? "?").AppendLine();
            sb.Append("Çıkış kodu: ").Append(exitCode)
              .Append(" (0x").Append(exitCode.ToString("X8", CultureInfo.InvariantCulture)).Append(')').AppendLine();
            sb.Append("Çalışma süresi: ").Append((int)runtime.TotalSeconds).Append(" sn").AppendLine();

            if (_outputLogPath != null)
                sb.Append("Oyun çıktısı: ").Append(_outputLogPath).AppendLine();

            if (crashReportPath != null)
            {
                sb.AppendLine();
                sb.AppendLine("--- crash-report (" + Path.GetFileName(crashReportPath) + ") ---");
                sb.AppendLine(ReadHead(crashReportPath, 120));
            }

            AppendGameLogTail(sb);

            sb.AppendLine();
            sb.AppendLine("--- oyun çıktısı (son satırlar) ---");
            sb.AppendLine(LastLines(tail, 120));

            return sb.ToString();
        }

        private void AppendGameLogTail(StringBuilder sb)
        {
            try
            {
                string latest = Path.Combine(_gameDirectory, "logs", "latest.log");

                if (!File.Exists(latest))
                    return;

                sb.AppendLine();
                sb.AppendLine("--- logs/latest.log (son satırlar) ---");
                sb.AppendLine(LastLines(ReadAllTextShared(latest), 80));
            }
            catch (Exception)
            {
                // Oyun dosyayı hâlâ açık tutuyor olabilir; rapor bu bölüm olmadan da işe yarar.
            }
        }

        private static string ReadAllTextShared(string path)
        {
            using (FileStream stream = new FileStream(path, FileMode.Open, FileAccess.Read, FileShare.ReadWrite))
            using (StreamReader reader = new StreamReader(stream, Encoding.UTF8))
            {
                return reader.ReadToEnd();
            }
        }

        private static string ReadHead(string path, int lineCount)
        {
            try
            {
                string[] lines = File.ReadAllLines(path);
                return string.Join(Environment.NewLine, lines.Take(lineCount));
            }
            catch (Exception)
            {
                return "(okunamadı)";
            }
        }

        private static string LastLines(string text, int lineCount)
        {
            if (string.IsNullOrEmpty(text))
                return "(çıktı yok)";

            string[] lines = text.Replace("\r\n", "\n").Split('\n');

            return lines.Length <= lineCount
                ? text
                : string.Join(Environment.NewLine, lines.Skip(lines.Length - lineCount));
        }
    }

    internal sealed class GameCrashEventArgs : EventArgs
    {
        internal GameCrashEventArgs(string hint, string details, string outputLogPath)
        {
            Hint = hint;
            Details = details;
            OutputLogPath = outputLogPath;
        }

        internal string Hint { get; private set; }

        internal string Details { get; private set; }

        internal string OutputLogPath { get; private set; }
    }
}
