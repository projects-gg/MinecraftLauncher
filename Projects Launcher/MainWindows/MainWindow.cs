using CmlLib.Core;
using CmlLib.Core.Auth;
using CmlLib.Core.Installers;
using CmlLib.Core.ProcessBuilder;
using DiscordRPC;
using Guna.UI2.WinForms.Enums;
using Microsoft.Win32;
using MineStatLib;
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.IO;
using System.Linq;
using System.Management;
using System.Net;
using System.Net.Http;
using System.Text;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace Projects_Launcher.Projects_Launcher
{
    public partial class mainMenuForm : Form
    {
        public mainMenuForm()
        {
            InitializeComponent();

            // TLS + bağlantı limiti bir kez ayarlanır (her indirmede tekrar set etmeye gerek yok).
            ServicePointManager.SecurityProtocol = SecurityProtocolType.Tls12;
            ServicePointManager.Expect100Continue = true;
            ServicePointManager.DefaultConnectionLimit = 256;

            StyleProgressBar();
            PopulateDropdowns();
        }

        // Tüm HTTP istekleri için tek paylaşılan istemci (soket tükenmesini önler, hızlı + asenkron).
        private static readonly HttpClient _http = new HttpClient { Timeout = TimeSpan.FromSeconds(20) };

        // timer1 (sunucu durumu) için yeniden giriş koruması.
        private bool _pinging;

        // İndirme çubuğuna modern görünüm: yatay gradyan dolgu, yuvarlatılmış uçlar, yumuşak gölge.
        private void StyleProgressBar()
        {
            downloadCompleteBar.ShowText = false; // eski "downloadCompleteBar" yazısını kaldırır
            downloadCompleteBar.FillColor = Color.FromArgb(40, 44, 52);        // koyu ray
            downloadCompleteBar.ProgressColor = Color.FromArgb(34, 197, 94);   // canlı yeşil
            downloadCompleteBar.ProgressColor2 = Color.FromArgb(16, 185, 129); // teal geçiş
            downloadCompleteBar.ProgressBrushMode = BrushMode.Gradient;
            downloadCompleteBar.GradientMode = LinearGradientMode.Horizontal;
            downloadCompleteBar.AutoRoundedCorners = true;                     // hap (pill) şekli
            downloadCompleteBar.ShadowDecoration.Enabled = true;
            downloadCompleteBar.ShadowDecoration.Color = Color.FromArgb(16, 185, 129);
            downloadCompleteBar.ShadowDecoration.Depth = 6;
        }

        // Sürüm/mod dropdown öğeleri Designer'da DEĞİL burada doldurulur.
        // Designer'a elle eklenen Items, form VS tasarımcısında açılınca yeniden üretim sırasında
        // siliniyor ve dropdown'lar boşalıyordu ("bi ara düzeldi, sonra yine bozuldu").
        // Kod-arkasında kalıcıdır; tasarımcı buraya dokunmaz.
        private void PopulateDropdowns()
        {
            versionBox.MaxDropDownItems = 5;
            versionBox.Items.Clear();
            versionBox.Items.AddRange(new object[]
            {
                "projects-fabric-" + latestFabricVersion,
                "26.1.2 ✔", "1.21.11", "1.21.8", "1.21.3", "1.21.1", "1.21",
                "1.20.4", "1.19.4", "1.18.2", "1.17.1", "1.16.5", "1.15.2",
                "1.14.4", "1.13.2", "1.12.2", "1.8.9", "1.7.10"
            });

            modVersionBox.MaxDropDownItems = 5;
            modVersionBox.Items.Clear();
            modVersionBox.Items.AddRange(new object[]
            {
                "projects-mcmod-" + latestModVersion,
                "Manuel"
            });

            temaSelectBox.MaxDropDownItems = 5;
        }

        private string sessions;
        private MSession session;
        private string minrambox;
        private string maxrambox;
        private string widthbox;
        private string heightbox;

        // Eskiden burada senkron HTTP açılışı donduruyordu. Artık Anamenu_Load arka planda günceller.
        // Başlangıç değerleri Settings'ten gelir; ağ yanıtı gelmeden ya da boş dönerse de sürüm
        // dolu kalır (ör. "projects-mcmod-" yerine "projects-mcmod-v12").
        public string latestFabricVersion = Properties.Settings.Default.latestFabric;

        public string latestModVersion = Properties.Settings.Default.lastModVer;

        string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);

        private string maxramlabell;
        private string minramlabell;
        private string heightlabell;
        private string widthlabell;
        private string surumlabell;
        private string ramInfo;

        private int widthResolution;
        private int heightResolution;
        private string heightResolutionb;
        private string heightResolutionb2;
        private string widthResolutionb;
        private string widthResolutionb2;

        public static String readPhpContent(String address)
        {
            try
            {
                string _newestVersion = "";
                WebRequest currentVersionContent = HttpWebRequest.Create(address);
                WebResponse versionContentResponse = currentVersionContent.GetResponse();
                StreamReader versionContentReader = new StreamReader(versionContentResponse.GetResponseStream());
                string versionContentLine = versionContentReader.ReadToEnd();
                bool startWriting = false;
                StringBuilder bld = new StringBuilder();

                foreach (char character in versionContentLine) //this is hard to read but culture-compatible
                {
                    if (character.Equals('>'))
                    {
                        if (!startWriting)
                        {
                            startWriting = true;
                        }
                    }
                    else if (startWriting)
                    {
                        if (!character.Equals('<'))
                        {
                            bld.Append(character);
                        }
                        else
                        {
                            break;
                        }
                    }
                }

                if (bld.Length >= 0)
                {
                    _newestVersion = bld.ToString();
                }

                return _newestVersion;
            }
            catch
            {
                return Properties.Settings.Default.latestFabric;
            }
        }

        private readonly string TextureDizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                               "/.projects/resourcepacks";

        private readonly string launcherdizin =
            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects";

        readonly Random _random = new Random();

        private bool alreadyPlayingAnimatedLabel;

        private readonly string currentVersion = Properties.Settings.Default.currentVersion;

        public DiscordRpcClient Client { get; private set; }

        private void DiscordRpcClientSetup()
        {
            try
            {
                Client = new DiscordRpcClient("949311557542756362");
                Client.Initialize();

                Client.SetPresence(new RichPresence
                {
                    Details = "Başlatıcı menüsünde",
                    State = "Sunucu IP: mc.projects.gg",
                    Assets = new Assets
                    {
                        LargeImageKey = "projects_logo",
                        LargeImageText = "https://mc.projects.gg/",
                        SmallImageKey = "world",
                    }
                });
            }
            catch
            {
                // Shouldn't happen except no internet connection or server downtime
            }
        }

        // WMI sorgusu yavaştır; arka planda çalıştırılıp sonuç UI thread'inde yazılır (açılış donmaz).
        private async Task UpdateHwInfoAsync()
        {
            try
            {
                string text = await Task.Run(() =>
                {
                    using (var ramSearch = new ManagementObjectSearcher("Select TotalPhysicalMemory From Win32_ComputerSystem"))
                    {
                        foreach (ManagementObject ramObject in ramSearch.Get())
                        {
                            double ramInBytes = Convert.ToDouble(ramObject["TotalPhysicalMemory"]);
                            double gb = Math.Ceiling(ramInBytes / 1073741824); // Byte -> GB
                            return string.Format("{0:0.##}", gb * 1024) + "MB" + "/" + Convert.ToString(gb) + " GB";
                        }
                    }
                    return "";
                });

                if (!string.IsNullOrEmpty(text))
                    ramInfoLabel.Text = text;
            }
            catch
            {
                // Donanım bilgisi alınamadı; kritik değil.
            }
        }

        private void thisFalse()
        {
            settingsBgPanel.Enabled = false;
            playButtonStaticLabel.Enabled = false;
            settingsStaticPictureBox.Enabled = false;
            discordStaticPictureBox.Enabled = false;
        }

        private void thisTrue()
        {
            settingsBgPanel.Enabled = true;
            playButtonStaticLabel.Enabled = true;
            settingsStaticPictureBox.Enabled = true;
            discordStaticPictureBox.Enabled = true;
        }

        private async void Anamenu_Load(object sender, EventArgs e)
        {
            versionLabel.Text = "v" + currentVersion;

            // ".projects" directory check
            if (!Directory.Exists(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                  "/.projects/versions"))
            {
                Directory.CreateDirectory(@Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                          "/.projects/versions");
            }

            DiscordRpcClientSetup();

            // --- Senkron UI kurulumu (hızlı, anında çizilir) ---
            playerNameStaticLabel.Text = Properties.Settings.Default.NickNames;
            autoConnect.Checked = Properties.Settings.Default.autoConnect;
            reopenLauncher.Checked = Properties.Settings.Default.reopenLauncher;
            temaSelectBox.Text = Properties.Settings.Default.themeSelected;

            if (Properties.Settings.Default.SelectedVersion != string.Empty)
            {
                versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;
                versionBox.Text = Properties.Settings.Default.SelectedVersion;
            }

            modVersionBox.Text = Properties.Settings.Default.curModVer;

            if (Properties.Settings.Default.RamMax != string.Empty)
            {
                maxRamTextBox.Text = Properties.Settings.Default.RamMax;
                maxRamMBtoGBLabel.Text = String.Format("{0:0.##}", Convert.ToDouble(maxRamTextBox.Text) / 1024) + "GB";
            }
            else if (maxRamDynamicCalculatorLabel.Text != "")
            {
                maxRamMBtoGBLabel.Text = "";
            }

            if (Properties.Settings.Default.RamMin != string.Empty)
            {
                minRamTextBox.Text = Properties.Settings.Default.RamMin;

                minRamMBtoGBLabel.Text =
                    String.Format("{0:0.##}", Convert.ToDouble(minRamTextBox.Text) / 1024) + "GB";
            }
            else if (maxRamDynamicCalculatorLabel.Text != "")
            {
                minRamMBtoGBLabel.Text = "";
            }

            minRamTextBox.MaxLength = 4;

            // Grab resolution data
            if (Properties.Settings.Default.ResolutionHeight != string.Empty)
            {
                widthtextbox.Text = Properties.Settings.Default.ResolutionHeight;
            }
            else if (Properties.Settings.Default.ResolutionWidth != string.Empty)
            {
                heighttextbox.Text = Properties.Settings.Default.ResolutionWidth;
            }

            // --- Ağ/WMI işleri arka planda (UI bloklanmaz) ---
            _ = UpdateHwInfoAsync();
            _ = onlineCountUpdater();

            // Sürüm bilgilerini arka planda çek. Yalnızca dolu yanıt gelirse üzerine yaz;
            // boş dönerse Settings'ten gelen değer korunur (sürüm asla boş kalmaz).
            await Task.Run(() =>
            {
                string fabric = readPhpContent("https://mc.projects.gg/LauncherUpdateStream/version-fabric.php");
                string mod = readPhpContent("https://mc.projects.gg/LauncherUpdateStream/version-mcmod.php");
                if (!string.IsNullOrWhiteSpace(fabric)) latestFabricVersion = fabric;
                if (!string.IsNullOrWhiteSpace(mod)) latestModVersion = mod;
            });

            // Skin render (asenkron; eskiden senkron GetResponse açılışı donduruyordu).
            try
            {
                using (var stream = await _http.GetStreamAsync("https://minotar.net/avatar/" + playerNameStaticLabel.Text))
                using (var ms = new MemoryStream())
                {
                    await stream.CopyToAsync(ms);
                    ms.Position = 0;
                    skinRenderPictureBox.Image = Bitmap.FromStream(ms);
                }
            }
            catch
            {
                // İnternet yoksa veya servis kapalıysa olur; kritik değil.
            }
        }

        private async Task LaunchGameAsync() // Minecraft başlatma (CmlLib 4.x)
        {
            var path = new MinecraftPath(launcherdizin);
            var launcher = new MinecraftLauncher(path);

            sessions = Properties.Settings.Default.NickNames;

            string serverIP = Properties.Settings.Default.autoConnect ? "play.projects.gg" : "";

            // Ayarlar güvenli parse edilir (boş/bozuk değerde çökmesin diye).
            if (!int.TryParse(Properties.Settings.Default.RamMin, out int minRam) || minRam <= 0)
                minRam = 1024;
            if (!int.TryParse(Properties.Settings.Default.RamMax, out int maxRam) || maxRam < minRam)
                maxRam = Math.Max(minRam, 2048);
            int.TryParse(Properties.Settings.Default.ResolutionWidth, out int screenWidth);
            int.TryParse(Properties.Settings.Default.ResolutionHeight, out int screenHeight);

            var ayarlar = new MLaunchOption
            {
                MinimumRamMb = minRam,
                MaximumRamMb = maxRam,
                Session = MSession.CreateOfflineSession(sessions),
                ServerIp = serverIP,
                GameLauncherName = "Projects Minecraft",
                ScreenWidth = screenWidth,
                ScreenHeight = screenHeight,
            };

            // İlerleme UI thread'ine marshal edilir (Progress<T> UI thread'inde oluşturuluyor).
            // Çubuk indirme yüzdesini (gradyan dolgu) gösterir, etiket dosya ilerlemesini.
            var fileProgress = new Progress<InstallerProgressChangedEventArgs>(p =>
            {
                downloadCompleteLabel.Text = p.TotalTasks > 0
                    ? $"İndiriliyor… {p.ProgressedTasks}/{p.TotalTasks} dosya"
                    : "Hazırlanıyor…";
            });
            var byteProgress = new Progress<ByteProgress>(p =>
            {
                if (p.TotalBytes > 0)
                    downloadCompleteBar.Value = (int)Math.Min(100L, p.ProgressedBytes * 100 / p.TotalBytes);
            });

            downloadCompleteLabel.Text = "Hazırlanıyor…";
            downloadCompleteLabel.Visible = true;
            downloadCompleteBar.Value = 0;
            downloadCompleteBar.Visible = true;

            // İndir + derle + başlat. Tamamen asenkron olduğundan UI donmaz.
            var clientStartProcess = await launcher.InstallAndBuildProcessAsync(
                Properties.Settings.Default.SelectedVersion, ayarlar, fileProgress, byteProgress, CancellationToken.None);

            clientStartProcess.Start(); // Oyunu başlat

            alreadyPlayingAnimatedLabel = false; // "Başlatılıyor..." animasyonunu durdur
            downloadCompleteBar.Visible = false;
            downloadCompleteLabel.Visible = false;

            this.Visible = false; // Oyun açılırken launcher gizlenir

            if (reopenLauncher.Checked)
            {
                // Oyun kapanınca launcher geri gelir (eski timer3 yoklaması yerine process.Exited olayı).
                clientStartProcess.EnableRaisingEvents = true;
                clientStartProcess.Exited += OnGameProcessExited;
            }
            else
            {
                // Yeniden açma kapalı: oyun başlayınca launcher kapanır (oyundayken Discord durumu gösterilmez).
                Application.Exit();
            }
        }

        // Oyun süreci sona erince (yalnızca "yeniden aç" açıkken) launcher'ı geri getirir.
        private void OnGameProcessExited(object sender, EventArgs e)
        {
            if (IsDisposed || !IsHandleCreated)
                return;

            try
            {
                BeginInvoke((Action)(() =>
                {
                    if (Properties.Settings.Default.SelectedVersion != string.Empty)
                        versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;

                    playButtonStaticLabel.Enabled = true;
                    this.Visible = true;
                    thisTrue();

                    Client?.Dispose();
                    DiscordRpcClientSetup();
                }));
            }
            catch (ObjectDisposedException)
            {
                // Launcher zaten kapanmış.
            }
        }

        private async void oynabutton_Click(object sender, EventArgs e)
        {
            string surum_appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                         "/.projects/versions/projects-fabric-" + latestFabricVersion; // Fabric directory

            // Güvenli parse: ayar boş/bozuksa varsayılan kullanılır (eskiden int.Parse çökerdi).
            if (!int.TryParse(Properties.Settings.Default.RamMin, out int MinimumRamMb)) MinimumRamMb = 1024;
            if (!int.TryParse(Properties.Settings.Default.RamMax, out int MaximumRamMb)) MaximumRamMb = 2048;

            if (MinimumRamMb > MaximumRamMb)
            {
                DialogResult ramExceptionResult = MessageBox.Show(
                    "Oyunu şu an başlatılamaz:\n\nVerilen azami RAM miktarı asgari\nmiktardan daha düşük.\n\nEşitleme yapılsın mı?\nTamam: Miktarları eşitle, oyunu başlat.\nİptal: Uyarıyı kapat ve oyunu başlatma.",
                    "Bilgi", MessageBoxButtons.OKCancel);

                if (ramExceptionResult == DialogResult.OK)
                {
                    Properties.Settings.Default.RamMax = Properties.Settings.Default.RamMin;
                }
                else
                {
                    return;
                }
            }

            if (!Properties.Settings.Default.curModVer.Equals("Manuel"))
            {
                if (!Properties.Settings.Default.SelectedVersion.Contains(Properties.Settings.Default.modCompatibleFabric) && !Properties.Settings.Default.SelectedVersion.Equals(Properties.Settings.Default.modCompatibleVer))
                {
                    DialogResult secenek = MessageBox.Show("Mod paketi yalnızca \"" + Properties.Settings.Default.modCompatibleFabric + "\" fabric sürümü ve \"" + Properties.Settings.Default.modCompatibleVer + "\" oyun sürümüyle uyumludur. Ayarlara girip mod seçiminizi Manuel yapmalısınız ya da bu sürümlerden birini seçip oyuna bağlanmalısınız.", "Uyumsuz Sürüm", MessageBoxButtons.OK); // Selected mod not compatible with selected version
                    return;
                }
            }

            if (Properties.Settings.Default.SelectedVersion.Contains("projects-fabric-"))
            {
                if (!Directory.Exists(@surum_appDataDizini))
                { //Check fabric is not exist
                    DialogResult secenek = MessageBox.Show("Yeni fabric sürümü yüklü değil. Ayarlardan sürümü değiştirebilirsiniz\nya da indirme başlatabilirsiniz. Eğer indirmek istiyorsanız\nonaylayınız.", "Eksik Dosya", MessageBoxButtons.YesNo); //Fabric dosyasının olmadığını bildir

                    if (secenek == DialogResult.Yes)
                    {
                        WebClient wc = new WebClient();
                        wc.DownloadFileCompleted +=
                            Wc_DownloadFileCompleted; // Call the codes when download process complete
                        wc.DownloadProgressChanged += Wc_DownloadProgressChanged;
                        Uri fabric = new Uri("https://mc.projects.gg/LauncherUpdateStream/projects-fabric-" + latestFabricVersion + ".zip"); // Fabric installer address
                        wc.DownloadFileAsync(fabric,
                            appDataDizini +
                            "/.projects/projects-fabric-" + latestFabricVersion + ".zip"); // Download fabric to directory '.projects'
                        playButtonStaticLabel.Enabled = false;
                        settingsStaticPictureBox.Enabled = false;
                        versionInfoStaticLabel.Text = "İndiriliyor...";
                        downloadCompleteLabel.Visible = true;
                        downloadCompleteBar.Visible = true;
                        playSplitStaticLabel.Visible = true;
                        Properties.Settings.Default.SelectedVersion = "projects-fabric-" + latestFabricVersion;
                        return;
                    } else
                    {
                        Properties.Settings.Default.SelectedVersion = "1.19.4";
                        versionBox.SelectedIndex = 2;
                        versionInfoStaticLabel.Text = "1.19.4";
                        MessageBox.Show("Oyun sürümünüz 1.19.4 olarak ayarlandı.\n\nFabric ile girmek isterseniz ayarlardan\nsürümü seçip oyunu başlatabilirsiniz.", "Sürüm Değiştirildi");
                    }
                }
            }

            try
            {
                Client.Dispose();
                Client = new DiscordRpcClient("949311557542756362");
                Client.Initialize();

                Client.SetPresence(new RichPresence
                {
                    Details = "Şu an oyunda!",
                    State = "Sunucu IP: mc.projects.gg",
                    Timestamps = new Timestamps
                    {
                        Start = DateTime.UtcNow
                    },
                    Assets = new Assets
                    {
                        LargeImageKey = "projects_logo",
                        LargeImageText = "https://mc.projects.gg/",
                        SmallImageKey = "world",
                    }
                });

                session = MSession.CreateOfflineSession(Properties.Settings.Default.NickNames); // Get nickname info

                thisFalse();
                if (!Directory.Exists(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                "/.projects/versions/" + Properties.Settings.Default.SelectedVersion.ToString()))
                {
                    if (versionBox.SelectedIndex == -1)
                    {
                        MessageBox.Show("Kullandığınız oyun sürümü \"" + Properties.Settings.Default.SelectedVersion + "\" yüklü değil!\n\nİlk defa yükleneceği için bu işlem\nbirkaç dakika sürebilir. Lütfen başlatıcıyı\nbu süreç içerisinde kapatmayınız.");
                        downloadCompleteBar.Visible = true;

                    }
                }

                if (!Properties.Settings.Default.curModVer.Equals("Manuel") && !Properties.Settings.Default.curModVer.Equals(latestModVersion))
                {
                    DialogResult askForModPackage = MessageBox.Show("Mod paketi güncel değil. Mod paketimizi kurarak mini harita, gece görüşü (H tuşu) gibi önemli özellikler elde edebilirsiniz. Reddetseniz bile daha sonra istemeniz durumunda ayarlar kısmından mod sürümü seçip mod paketi kurabilirsiniz.\n\nYüklemek istiyor musunuz?", "Mod Paketi", MessageBoxButtons.YesNo);

                    if (askForModPackage == DialogResult.Yes)
                    {
                        updateModPackage();
                        return;
                    } else if (Properties.Settings.Default.modDontAsk != latestModVersion)
                    {
                        DialogResult askToDontAsk = MessageBox.Show("Bir sonraki sürüme kadar mod güncelleme bildirimlerini kapatmak ister misiniz?", "Güncelleme Bildirimleri", MessageBoxButtons.YesNo);

                        if (askToDontAsk == DialogResult.Yes)
                        {
                            Properties.Settings.Default.modDontAsk = latestModVersion;
                        }
                    }
                }

                _ = animatedPlayingLabel(); // "Başlatılıyor..." animasyonu
                await LaunchGameAsync();    // İndir + derle + başlat (UI donmadan)
            }
            catch (Exception ex)
            {
                alreadyPlayingAnimatedLabel = false; // animasyonu durdur
                DiscordRpcClientSetup();

                NotificationAboutException(ex, "Oyun başlatma");

                versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;
                downloadCompleteBar.Visible = false;
                downloadCompleteLabel.Visible = false;

                thisTrue(); // Launcher bileşenlerini tekrar aç
            }
        }

        private void updateModPackage()
        {
            WebClient wc2 = new WebClient();

            if (Directory.Exists(appDataDizini + "/.projects/mods"))
            {
                Directory.Delete(appDataDizini + "/.projects/mods", true);
            }

            wc2.DownloadFileCompleted +=
                Wc2_DownloadFileCompleted; // Call the codes when download process complete

            wc2.DownloadProgressChanged += Wc_DownloadProgressChanged;

            Uri modVersion = new Uri("https://mc.projects.gg/LauncherUpdateStream/projects-mcmod-" + latestModVersion + ".zip"); // Mod installer address

            Properties.Settings.Default.curModVer = latestModVersion;
            Properties.Settings.Default.Save();

            playButtonStaticLabel.Enabled = false;
            settingsStaticPictureBox.Enabled = false;
            versionInfoStaticLabel.Text = "Mod paketi kuruluyor...";
            downloadCompleteLabel.Visible = true;
            downloadCompleteBar.Visible = true;
            playSplitStaticLabel.Visible = true;

            wc2.DownloadFileAsync(modVersion,
                appDataDizini +
                "/.projects/projects-mcmod-" + latestModVersion + ".zip"); // Download fabric to directory '.projects
        }

        private void Wc_DownloadProgressChanged(object sender, DownloadProgressChangedEventArgs e)
        {
            downloadCompleteLabel.Text = String.Format("{0:0.##}", Convert.ToDouble(e.BytesReceived) / 1024 / 1024) +
                                         "MB"
                                         + "/" + String.Format("{0:0.##}",
                                             Convert.ToDouble(e.TotalBytesToReceive) / 1024 / 1024) + "MB";

            downloadCompleteBar.Value = e.ProgressPercentage;
        }

        private void Wc2_DownloadFileCompleted(object sender, AsyncCompletedEventArgs e)
        {
            try
            {
                string zipPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                 "/.projects/projects-mcmod-" + latestModVersion + ".zip";
                string extractPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                     "/.projects/mods/";

                System.IO.Compression.ZipFile.ExtractToDirectory(zipPath, extractPath);
                Properties.Settings.Default.curModVer = latestModVersion;
                Properties.Settings.Default.Save();
                playButtonStaticLabel.Enabled = true;
                settingsStaticPictureBox.Enabled = true;
                downloadCompleteLabel.Visible = false;
                downloadCompleteBar.Visible = false;
                playSplitStaticLabel.Visible = false;
                versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;
            }
            catch (Exception ex)
            {
                NotificationAboutException(ex, "DownloadFileCompleted (mod download process)");
            }
        }

        private void Wc_DownloadFileCompleted(object sender, AsyncCompletedEventArgs e)
        {
            try
            {
                string zipPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                 "/.projects/projects-fabric-" + latestFabricVersion + ".zip";
                string extractPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                     "/.projects/versions";

                System.IO.Compression.ZipFile.ExtractToDirectory(zipPath, extractPath);
                Properties.Settings.Default.SelectedVersion = "projects-fabric-" + latestFabricVersion;
                versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;
                playButtonStaticLabel.Enabled = true;
                settingsStaticPictureBox.Enabled = true;
                downloadCompleteLabel.Visible = false;
                downloadCompleteBar.Visible = false;
                playSplitStaticLabel.Visible = false;
            }
            catch (Exception ex)
            {
                NotificationAboutException(ex, "DownloadFileCompleted (fabric download process)");
            }

            if (!Properties.Settings.Default.curModVer.Equals("Manuel")) {
                if (!Properties.Settings.Default.curModVer.Equals(latestModVersion) || !Directory.Exists(appDataDizini + "/.projects/mods")) {
                    DialogResult askForModPackage = MessageBox.Show("Mod paketi güncel değil. Mod paketimizi kurarak mini harita, gece görüşü (H tuşu) gibi önemli özellikler elde edebilirsiniz. Reddetseniz bile daha sonra istemeniz durumunda ayarlar kısmından mod sürümü seçip mod paketi kurabilirsiniz.\n\nYüklemek istiyor musunuz?", "Mod Paketi", MessageBoxButtons.YesNo); //Fabric dosyasının olmadığını bildir

                    if (askForModPackage == DialogResult.Yes) {
                        updateModPackage();
                        return;
                    }
                }
            }
        }

        private void prepareGameToLaunch_Tick(object sender, EventArgs e)
        {
            // Eski javaw yoklaması kaldırıldı: Process.Start("javaw.exe") tam yol olmadan çağrılıyordu;
            // java'yı PATH'e kurmamış kullanıcılarda "Sistem belirtilen dosyayı bulamıyor" (Win32 hata 2)
            // hatasının kaynağı buydu ve gereksizdi. Oyun süreci artık LaunchGameAsync içinde
            // process.Exited olayı ile izleniyor; bu zamanlayıcı kullanılmıyor.
        }

        private void NotificationAboutException(Exception ex, string location = "\n")
        {
            if (!location.Equals("\n"))
                location = "\n\nHata konumu: " + location;
            MessageBox.Show(
                "Başlatıcı görevi işlenirken beklenmedik bir hata oluştu.\n\nBu hata önemli olmayabilir ya da programın yanlış çalışmasına neden oluyor olabilir. Eğer sorun yaşıyorsanız uygulamayı yeniden başlatın. Hata devam ederse destek sisteminde hatayı bizimle paylaşın." +
                location + "\nHata kodu: " +
                Convert.ToString(ex), "Başlatıcı Hatası");
        }

        private async Task animatedPlayingLabel()
        {
            if (alreadyPlayingAnimatedLabel)
            {
                return;
            }

            alreadyPlayingAnimatedLabel = true;

            do
            {
                if (versionInfoStaticLabel.Text.Equals("Başlatılıyor"))
                {
                    versionInfoStaticLabel.Text = "Başlatılıyor.";
                }
                else if (versionInfoStaticLabel.Text.Equals("Başlatılıyor."))
                {
                    versionInfoStaticLabel.Text = "Başlatılıyor..";
                }
                else if (versionInfoStaticLabel.Text.Equals("Başlatılıyor.."))
                {
                    versionInfoStaticLabel.Text = "Başlatılıyor...";
                }
                else
                {
                    versionInfoStaticLabel.Text = "Başlatılıyor";
                }

                await Task.Delay(500).ConfigureAwait(true);
            } while (alreadyPlayingAnimatedLabel);
        }

        private void ayarlarbutton_Click(object sender, EventArgs e)
        {
            if (settingsBgPanel.Visible == false)
            {
                backButton.Visible = true;
                settingsBgPanel.Visible = true;
            }
            else
            {
                settingsBgPanel.Visible = false;
            }
        }

        private async Task onlineCountUpdater()
        {
            do
            {
                string text;
                try
                {
                    // DNS çözümleme + sunucu ping'i bloklayıcıdır; arka planda çalıştırılır.
                    text = await Task.Run(() =>
                    {
                        IPHostEntry proxyIP = Dns.GetHostEntry(Properties.Settings.Default.ProxyIP);
                        string anyIP = proxyIP.AddressList.FirstOrDefault()?.ToString() ?? "";
                        var pinger = new MineStat(anyIP, 25565);
                        return pinger.ServerUp ? pinger.CurrentPlayers + " kişi oynuyor!" : "Bağlantı Yok";
                    });
                }
                catch
                {
                    text = "Bağlantı Yok";
                }

                // ConfigureAwait yok: devam UI thread'inde kalır, etiket cross-thread olmadan güncellenir.
                if (IsDisposed) return; // form kapandıysa döngüyü bitir (sızıntıyı önle)
                serverOnlineCountStaticLabel.Text = text;

                await Task.Delay(5000);
            } while (!IsDisposed);
        }

        private void ramlabel_Click(object sender, EventArgs e)
        {
            maxramlabell = maxramlabel.Text;
        }

        private void widthlabel_Click(object sender, EventArgs e)
        {
            widthlabell = widthlabel.Text;
        }

        private void heightlabel_Click(object sender, EventArgs e)
        {
            heightlabell = heightlabel.Text;
        }

        private void surumtext_Click(object sender, EventArgs e)
        {
            surumlabell = surumtext.Text;
        }

        private void widthtextbox_TextChanged(object sender, EventArgs e)
        {
            widthbox = widthtextbox.Text;
            Properties.Settings.Default.ResolutionWidth = widthbox;
            Properties.Settings.Default.Save();
            widthlabell = Properties.Settings.Default.ResolutionWidth;
        }

        private void heighttextbox_TextChanged(object sender, EventArgs e)
        {
            heightbox = heighttextbox.Text;
            Properties.Settings.Default.ResolutionHeight = heightbox;
            Properties.Settings.Default.Save();
            heightlabell = Properties.Settings.Default.ResolutionHeight;
        }

        private void maxramtext_TextChanged(object sender, EventArgs e)
        {
            try
            {
                maxrambox = maxRamTextBox.Text;
                Properties.Settings.Default.RamMax = maxrambox;
                Properties.Settings.Default.Save();
                maxramlabel.Text = Properties.Settings.Default.RamMax;

                //GB Convert
                if (Properties.Settings.Default.RamMax != string.Empty)
                {
                    maxramlabel.Text = Properties.Settings.Default.RamMax;
                    try
                    {
                        maxRamMBtoGBLabel.Text =
                            String.Format("{0:0.##}", Convert.ToDouble(maxRamTextBox.Text) / 1024) + "GB";
                    }
                    catch
                    {
                    }
                }
                else if (maxRamMBtoGBLabel.Text != "")
                {
                    maxRamMBtoGBLabel.Text = "";
                }
            }
            catch
            {
                MessageBox.Show("RAM miktarı ayarlanırken bir hata meydana geldi.");
            }
        }

        private void surumsec_SelectedIndexChanged(object sender, EventArgs e)
        {
            string testString = versionBox.Text, resultString = "";

            if (testString.IndexOf("") == -1)
            {
                resultString = testString;
            }
            else
            {
                foreach (char versionTextChars in testString)
                {
                    if (versionTextChars.Equals(' '))
                    {
                        break;
                    }

                    resultString += versionTextChars;
                }
            }

            Properties.Settings.Default.SelectedVersion = resultString;
            Properties.Settings.Default.Save();
            versionInfoStaticLabel.Text = versionBox.Text;
        }

        private void modPickVersion_SelectedIndexChanged(object sender, EventArgs e)
        {
            string testString = modVersionBox.Text, resultString = "";

            if (testString.IndexOf("") == -1)
            {
                resultString = testString;
            }
            else
            {
                foreach (char versionTextChars in testString)
                {
                    if (versionTextChars.Equals(' '))
                    {
                        break;
                    }

                    resultString += versionTextChars;
                }
            }

            Properties.Settings.Default.curModVer = resultString;
            Properties.Settings.Default.Save();
        }

        private void discord_Click(object sender, EventArgs e)
        {
            try
            {
                System.Diagnostics.Process.Start("https://projects.gg/discord");
            }
            catch
            {
                // Shouldn't happen except no internet connection or server downtime
            }
        }

        private void minramtext_TextChanged(object sender, EventArgs e)
        {
            try
            {
                minrambox = minRamTextBox.Text;
                Properties.Settings.Default.RamMin = minrambox;
                Properties.Settings.Default.Save();
                minramlabel.Text = Properties.Settings.Default.RamMin;

                //GB Convert
                if (Properties.Settings.Default.RamMin != string.Empty)
                {
                    try
                    {
                        minRamMBtoGBLabel.Text =
                            String.Format("{0:0.##}", Convert.ToDouble(minRamTextBox.Text) / 1024) + "GB";
                    }
                    catch
                    {
                    }
                }
                else if (minRamMBtoGBLabel.Text != "")
                {
                    minRamMBtoGBLabel.Text = "";
                }
            }
            catch
            {
                MessageBox.Show("RAM miktarı ayarlanırken bir hata meydana geldi.");
            }
        }

        private void minramlabel_Click(object sender, EventArgs e)
        {
            if (!string.IsNullOrEmpty(minramlabel.Text))
            {
                minramlabell = minramlabel.Text;
            }
        }

        private void modsLabel_Click(object sender, EventArgs e)
        {
            try
            {
                Directory.CreateDirectory(appDataDizini); // varsa dokunmaz
                Process.Start(appDataDizini);             // klasörü Gezgin'de aç
            }
            catch (Exception ex)
            {
                NotificationAboutException(ex, "Klasör açma");
            }
        }

        private void rpTransfer_Click(object sender, EventArgs e)
        {
            Directory.CreateDirectory(TextureDizin); // varsa dokunmaz

            using (OpenFileDialog file = new OpenFileDialog())
            {
                file.Filter = "ZIP Dosyası |*.zip";
                file.FilterIndex = 2;
                file.RestoreDirectory = true;
                file.CheckFileExists = false;
                file.Title = "ZIP Dosyası Seçiniz.";

                if (file.ShowDialog() != DialogResult.OK)
                    return;

                string DosyaYolu = file.FileName;
                string DosyaAdi = file.SafeFileName;
                if (!string.IsNullOrEmpty(DosyaAdi) && !string.IsNullOrEmpty(DosyaYolu))
                {
                    if (File.Exists(TextureDizin + "\\" + DosyaAdi))
                    {
                        MessageBox.Show(DosyaAdi + " isimli doku paketi zaten mevcut.", "", MessageBoxButtons.OK,
                            MessageBoxIcon.Warning);
                    }
                    else
                    {
                        File.Copy(DosyaYolu, TextureDizin + "\\" + DosyaAdi);
                        MessageBox.Show("Doku paketi başarıyla yüklendi.");
                    }
                }
            }
        }

        private void rpFolder_Click(object sender, EventArgs e)
        {
            try
            {
                Directory.CreateDirectory(TextureDizin); // varsa dokunmaz
                Process.Start(TextureDizin);
            }
            catch (Exception ex)
            {
                NotificationAboutException(ex, "Klasör açma");
            }
        }

        private void gamefolder_Click(object sender, EventArgs e)
        {
            try
            {
                string dir = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects";
                Directory.CreateDirectory(dir); // varsa dokunmaz
                Process.Start(dir);
            }
            catch (Exception ex)
            {
                NotificationAboutException(ex, "Klasör açma");
            }
        }

        private void timer3_Tick(object sender, EventArgs e)
        {
            // Oyun kapanışını izleyen eski yoklama döngüsü kaldırıldı: do/while içinde UI thread'i
            // bloke ediliyordu (donma). Yeniden açma artık OnGameProcessExited ile olay tabanlı.
        }

        private void maxramtext_Leave(object sender, EventArgs e)
        {
            try
            {
                ManagementObjectSearcher getRAM = new ManagementObjectSearcher("Select * From Win32_ComputerSystem");

                foreach (ManagementObject Mobject in getRAM.Get())
                {
                    double Ram_Bytes = (Convert.ToDouble(Mobject["TotalPhysicalMemory"]));
                    double ramgb = Ram_Bytes / 1073741824;
                    double islem = Math.Ceiling(ramgb);
                    ramInfo = String.Format("{0:0.##}", (Convert.ToDouble(islem) * 1024) - 1024);
                    break;
                }

                maxRamTextBox.Text = (maxRamTextBox.Text).Trim();
                if (string.IsNullOrEmpty(maxRamTextBox.Text))
                {
                    MessageBox.Show(Convert.ToInt32(minRamTextBox.Text) + "-" + ramInfo + " " + "arasında girilmeli.");
                    maxRamTextBox.Text = ramInfo;
                }
                else if (Convert.ToInt32(maxRamTextBox.Text) < Convert.ToInt32(minRamTextBox.Text) ||
                         Convert.ToInt32(maxRamTextBox.Text) > Convert.ToInt32(ramInfo))
                {
                    MessageBox.Show(Convert.ToInt32(minRamTextBox.Text) + "-" + ramInfo + " " + "arasında girilmeli.");
                    maxRamTextBox.Text = ramInfo;
                }
                else if (Convert.ToInt32(ramInfo) >= Convert.ToInt32(minRamTextBox.Text) &&
                         Convert.ToInt32(maxRamTextBox.Text) > Convert.ToInt32(ramInfo) - 512)
                {
                    MessageBox.Show(
                        "Yüksek kaynak kullanımı!\n\nYüksek kaynak tüketimi bilgisayarınızdaki\nbazı şeylerin yavaş çalışmasına neden olabilir\nEn yüksek RAM miktarınızı, azami RAM\nmiktarından daha az tutmanız tavsiye\nedilir.",
                        "Kaynak Tüketim Uyarısı");
                    maxRamTextBox.Text = ramInfo;
                }
            }
            catch
            {
                MessageBox.Show("RAM miktarı ayarlanırken bir hata meydana geldi.");
            }
        }

        private void minramtext_Leave(object sender, EventArgs e)
        {
            try
            {
                ManagementObjectSearcher Search = new ManagementObjectSearcher("Select * From Win32_ComputerSystem");
                foreach (ManagementObject Mobject in Search.Get())
                {
                    double Ram_Bytes = (Convert.ToDouble(Mobject["TotalPhysicalMemory"]));
                    double ramgb = Ram_Bytes / 1073741824;
                    double islem = Math.Ceiling(ramgb);
                    ramInfo = String.Format("{0:0.##}", Convert.ToDouble(islem) * 512);
                }

                minRamTextBox.Text = (minRamTextBox.Text).Trim();
                if (string.IsNullOrEmpty(minRamTextBox.Text) || Convert.ToInt32(minRamTextBox.Text) < 1024 ||
                    Convert.ToInt32(minRamTextBox.Text) > Convert.ToInt32(ramInfo))
                {
                    MessageBox.Show("Miktar 1024-" + ramInfo + " " + "arasında girilmeli.");
                    minRamTextBox.Text = ramInfo;
                }
            }
            catch
            {
                MessageBox.Show("RAM miktarı ayarlanırken bir hata meydana geldi.");
            }
        }

        private void heighttextbox_Leave(object sender, EventArgs e)
        {
            try
            {
                heightResolution = Screen.PrimaryScreen.Bounds.Height;
                heightResolutionb = String.Format("{0:0.##}", Convert.ToDouble(heightResolution) / 2);
                heightResolutionb2 = String.Format("{0:0.##}", Convert.ToDouble(heightResolutionb) / 2);

                heighttextbox.Text = (heighttextbox.Text).Trim();
                if (string.IsNullOrEmpty(heighttextbox.Text))
                {
                    MessageBox.Show("Çözünürlük" + " " + heightResolutionb2 + "-" + heightResolutionb + " " +
                                    "arasında girilmeli.");
                    heighttextbox.Text = heightResolutionb;
                }
                else if (Convert.ToInt32(heighttextbox.Text) < Convert.ToInt32(heightResolutionb2) ||
                         Convert.ToInt32(heighttextbox.Text) > Convert.ToInt32(heightResolutionb))
                {
                    MessageBox.Show("Çözünürlük" + " " + heightResolutionb2 + "-" + heightResolutionb + " " +
                                    "arasında girilmeli.");
                    heighttextbox.Text = heightResolutionb2;
                }
            }
            catch
            {
                MessageBox.Show("Çözünürlük ayarlanırken bir hata meydana geldi.");
            }
        }

        private void widthtextbox_Leave(object sender, EventArgs e)
        {
            try
            {
                widthResolution = Screen.PrimaryScreen.Bounds.Width;
                widthResolutionb = String.Format("{0:0.##}", Convert.ToDouble(widthResolution) / 2);
                widthResolutionb2 = String.Format("{0:0.##}", Convert.ToDouble(widthResolutionb) / 2);

                widthtextbox.Text = (widthtextbox.Text).Trim();
                if (string.IsNullOrEmpty(widthtextbox.Text))
                {
                    MessageBox.Show("Çözünürlük" + " " + widthResolutionb2 + "-" + widthResolutionb + " " +
                                    "arasında girilmeli.");
                    widthtextbox.Text = widthResolutionb;
                }

                if (Convert.ToInt32(widthtextbox.Text) < Convert.ToInt32(widthResolutionb2) ||
                    Convert.ToInt32(widthtextbox.Text) > Convert.ToInt32(widthResolutionb))
                {
                    MessageBox.Show("Çözünürlük" + " " + widthResolutionb2 + "-" + widthResolutionb + " " +
                                    "arasında girilmeli.");
                    widthtextbox.Text = widthResolutionb;
                }
            }
            catch
            {
                MessageBox.Show("Çözünürlük ayarlanırken bir hata meydana geldi.");
            }
        }

        private void guna2ControlBox3_Click(object sender, EventArgs e)
        {
            Environment.Exit(0);
        }

        private void guna2ControlBox2_Click(object sender, EventArgs e)
        {
            this.WindowState = FormWindowState.Minimized;
        }

        private void reopenLauncherCheckBox_MouseEnter(object sender, EventArgs e)
        {
            ToolTip discordRpcTip = new ToolTip();
            discordRpcTip.UseFading = true;
            discordRpcTip.UseAnimation = true;
            discordRpcTip.IsBalloon = true;
            discordRpcTip.ShowAlways = true;
            discordRpcTip.SetToolTip(this.reopenLauncher,
                "Oyun kapatıldığında yeniden açılıp açılmayacağını seçersiniz.\n\nBu özelliğin kapalı olması durumunda oyundayken, discord\noynuyor bilginiz oyun durumunuzda gözükmeyecektir.");
        }

        private void autoConnectCheckBox_MouseEnter(object sender, EventArgs e)
        {
            ToolTip autoConnectTip = new ToolTip();
            autoConnectTip.UseFading = true;
            autoConnectTip.UseAnimation = true;
            autoConnectTip.IsBalloon = true;
            autoConnectTip.ShowAlways = true;
            autoConnectTip.SetToolTip(this.autoConnect,
                "Minecraft istemcisi açıldıktan sonra otomatik olarak Projects\nresmi sunucusuna girip girmeyeceğini seçebilirsiniz.\n\nAyarın kapalı olması durumunda Minecraft ana menüsü\naçılacaktır.");
        }

        private void guna2ControlBox1_Resize(object sender, EventArgs e)
        {
            settingsBgPanel.Size = this.Size;
        }

        private void button1_Click(object sender, EventArgs e)
        {
            settingsBgPanel.Visible = false;
            backButton.Visible = false;
        }

        private async void timer1_Tick(object sender, EventArgs e)
        {
            if (_pinging) return; // önceki istek bitmeden tekrar girme (5 sn'lik tick'ler birikmesin)
            _pinging = true;
            try
            {
                // Sunucu durumları asenkron çekilir; eskiden senkron GetResponse her 5 sn UI'yi donduruyordu.
                string lobi = await _http.GetStringAsync("https://projectsggapi.vercel.app/api/server1");
                lobiOnline.Image = lobi.Contains("\"online\":false") ? Properties.Resources.De_Aktif : Properties.Resources.Aktif;
            }
            catch
            {
                lobiOnline.Text = "?";
            }
            finally
            {
                _pinging = false;
            }
        }

        private void temaSelectBox_SelectedIndexChanged(object sender, EventArgs e)
        {

            if (temaSelectBox.Text == "Açık Tema")
            {
                this.BackgroundImage = Properties.Resources.gaia_light;

                minRamDynamicCalculatorLabel.ForeColor = Color.Black;
                maxRamDynamicCalculatorLabel.ForeColor = Color.Black;

                reopenLauncher.ForeColor = Color.Black;
                autoConnect.ForeColor = Color.Black;

                this.Icon = Properties.Resources.ProjectsLauncherLogo_dark;
            }
            else if (temaSelectBox.Text == "Koyu Tema")
            {
                this.BackgroundImage = Properties.Resources.gaia_dark;

                minRamDynamicCalculatorLabel.ForeColor = Color.FromArgb(251, 255, 255);
                maxRamDynamicCalculatorLabel.ForeColor = Color.FromArgb(251, 255, 255);
                reopenLauncher.ForeColor = Color.FromArgb(251, 255, 255);
                autoConnect.ForeColor = Color.FromArgb(251, 255, 255);
                versionBox.ForeColor = Color.FromArgb(251, 255, 255);

                this.Icon = Properties.Resources.ProjectsLauncherLogo_light;
            }
            else
            {
                int res = 1;

                try
                {
                    res = (int) Registry.GetValue(
                        "HKEY_CURRENT_USER\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
                        "AppsUseLightTheme", -1);
                }
                catch
                {
                    // res already have a default value.
                }

                if (res == 1)
                {
                    this.BackgroundImage = Properties.Resources.gaia_light;

                    minRamDynamicCalculatorLabel.ForeColor = Color.Black;
                    maxRamDynamicCalculatorLabel.ForeColor = Color.Black;

                    reopenLauncher.ForeColor = Color.Black;
                    autoConnect.ForeColor = Color.Black;

                    this.Icon = Properties.Resources.ProjectsLauncherLogo_dark;
                }

                if (res == 0)
                {
                    this.BackgroundImage = Properties.Resources.gaia_dark;

                    minRamDynamicCalculatorLabel.ForeColor = Color.FromArgb(251, 255, 255);
                    maxRamDynamicCalculatorLabel.ForeColor = Color.FromArgb(251, 255, 255);
                    reopenLauncher.ForeColor = Color.FromArgb(251, 255, 255);
                    autoConnect.ForeColor = Color.FromArgb(251, 255, 255);
                    versionBox.ForeColor = Color.FromArgb(251, 255, 255);

                    this.Icon = Properties.Resources.ProjectsLauncherLogo_light;
                }
            }

            Properties.Settings.Default.Save();
        }

        private void autoConnect_CheckedChanged(object sender, EventArgs e)
        {
            Properties.Settings.Default.autoConnect = autoConnect.Checked;
            Properties.Settings.Default.Save();
        }

        private void reopenLauncherCheckBox_CheckedChanged(object sender, EventArgs e)
        {
            Properties.Settings.Default.reopenLauncher = reopenLauncher.Checked;
            Properties.Settings.Default.Save();
        }
    }
}
