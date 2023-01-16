using CmlLib.Core;
using CmlLib.Core.Auth;
using DiscordRPC;
using Microsoft.Win32;
using MineStatLib;
using Newtonsoft.Json;
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Management;
using System.Net;
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
        }

        private string sessions;
        private MSession session;
        private string minrambox;
        private string maxrambox;
        private string widthbox;
        private string heightbox;
        
        public string latestFabricVersion =
            readPhpContent("https://mc.projects.gg/LauncherUpdateStream/version-fabric.php");

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

                Properties.Settings.Default.latestRealFabric = _newestVersion;

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
        private bool alreadyRelaunchWaiting;

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

        private void updateHwInfo()
        {
            // RAM
            ManagementObjectSearcher ramSearch = new ManagementObjectSearcher("Select * From Win32_ComputerSystem");

            foreach (ManagementObject ramObject in ramSearch.Get())
            {
                double ramInBytes = (Convert.ToDouble(ramObject["TotalPhysicalMemory"]));
                double roundAvailableRamValueInGb = Math.Ceiling(ramInBytes / 1073741824); // <- Byte to GB conversion
                ramInfoLabel.Text = string.Format("{0:0.##}", Convert.ToDouble(roundAvailableRamValueInGb) * 1024) +
                                    "MB" + "/" + Convert.ToString(roundAvailableRamValueInGb) + " GB";
                break;
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

        private void Anamenu_Load(object sender, EventArgs e)
        {
            versionLabel.Text = "v" + currentVersion;

            // ".projects" directory check
            if (!Directory.Exists(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                  "/.projects/versions"))
            {
                Directory.CreateDirectory(@Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                          "/.projects/versions");
            }

            updateHwInfo();

            DiscordRpcClientSetup();

            onlineCountUpdater().GetAwaiter();

            playerNameStaticLabel.Text = Properties.Settings.Default.NickNames;

            reopenLauncher.Checked = Properties.Settings.Default.OyunTickS;

            temaSelectBox.Text = Properties.Settings.Default.themeSelected;

            if (Properties.Settings.Default.SelectedVersion != string.Empty)
            {
                versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;
                versionBox.Text = Properties.Settings.Default.SelectedVersion;
            }

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

            // Grab skin render
            try
            {
                var request = WebRequest.Create("https://minotar.net/avatar" + "/" + playerNameStaticLabel.Text);

                using (var response = request.GetResponse())
                using (var stream = response.GetResponseStream())
                {
                    skinRenderPictureBox.Image = Bitmap.FromStream(stream);
                }
            }
            catch
            {
                // Shouldn't happen except no internet connection or server downtime
            }

            GC.WaitForPendingFinalizers();

            DataBindings.Clear();
            GC.SuppressFinalize(this);
        }

        private async Task Launch() // Minecraft startup settings
        {
            var path = new MinecraftPath(launcherdizin);
            var launcher = new CMLauncher(path);

            sessions = Properties.Settings.Default.NickNames;

            string serverIP = Properties.Settings.Default.autoConnect ? "play.projects.gg" : "";

            var ayarlar = new MLaunchOption
            {
                MinimumRamMb = int.Parse(Properties.Settings.Default.RamMin), // Get maximum ram info
                MaximumRamMb = int.Parse(Properties.Settings.Default.RamMax), // Get minimum ram info
                Session = MSession.GetOfflineSession(sessions), // Get nickname info
                ServerIp = serverIP, // The server IP which should connected
                GameLauncherName = "Projects Minecraft",
                ScreenWidth = int.Parse(Properties.Settings.Default.ResolutionWidth), // Get width resolution info
                ScreenHeight = int.Parse(Properties.Settings.Default.ResolutionHeight), // Get height resolution info
            };
            try
            {
                // Maximize download speed
                System.Net.ServicePointManager.DefaultConnectionLimit = 256;

                // Avoid SSL/TLS bridge error on Windows 7/XP
                ServicePointManager.Expect100Continue = true;
                ServicePointManager.SecurityProtocol = SecurityProtocolType.Tls12;

                var clientStartProcess =
                    await launcher.CreateProcessAsync(Properties.Settings.Default.SelectedVersion,
                        ayarlar); // Start client

                clientStartProcess.Start(); // Launch the game

                //prepareGameToLaunch.Enabled = true; // Launch prepareGameToLaunch
            }
            catch (Exception ex)
            {
                //ex.Equals(Exception as System.Collections.Generic.KeyNotFoundException)
                NotificationAboutException(ex, "Minecraft startup settings");
            }
        }

        private void oynabutton_Click(object sender, EventArgs e)
        {
            string surum_appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                         "/.projects/versions/projects-fabric-" + latestFabricVersion; // Fabric directory
            string appDataDizini =
                Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData); // AppData directory

            int MinimumRamMb = int.Parse(Properties.Settings.Default.RamMin);
            int MaximumRamMb = int.Parse(Properties.Settings.Default.RamMax);

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

            Uri fabric =
                new Uri(
                    "https://mc.projects.gg/LauncherUpdateStream/projects-fabric-" + latestFabricVersion + ".zip"); // Fabric installer address

            if (Directory.Exists(@surum_appDataDizini)) //Check fabric is exist
            {
                try //If fabric exists
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

                    session = MSession.GetOfflineSession(Properties.Settings.Default.NickNames); // Get nickname info

                    thisFalse();
                    if (!Directory.Exists(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                           "/.projects/versions/" + Properties.Settings.Default.SelectedVersion.ToString()))
                    {
                        if (versionBox.SelectedIndex == -1)
                        {
                            MessageBox.Show("Kullandığınız oyun sürümü \"" + Properties.Settings.Default.SelectedVersion + "\" yüklü değil!\n\nİlk defa yükleneceği için bu işlem\nbirkaç dakika sürebilir. Lütfen başlatıcıyı\nbu süreç içerisinde kapatmayınız.");
                        }
                    }

                    Thread thread = new Thread(() => Launch().GetAwaiter());
                    thread.IsBackground = true;
                    thread.Start(); // Launch the game
                    animatedPlayingLabel().GetAwaiter();
                    prepareGameToLaunch.Start(); // Launch prepareGameToLaunch
                }
                catch (Exception ex) //If fabric not exist
                {
                    DiscordRpcClientSetup();

                    prepareGameToLaunch.Stop(); // Stop prepareGameToLaunch
                    NotificationAboutException(ex, "Luanch prepareGameToLaunch");

                    thisTrue(); // Open components of the launcher

                    versionInfoStaticLabel.Text =
                        Properties.Settings.Default
                            .SelectedVersion; //Write version info into versionInfoStaticLabel

                    thisTrue();
                }
            }
            else
            {
                DialogResult secenek = MessageBox.Show("Projects Fabric bulunamadı! İndirmek ister misiniz?",
                    "Projects Fabric Dosyası Eksik", MessageBoxButtons.YesNo); //Fabric dosyasının olmadığını bildir

                if (secenek == DialogResult.Yes)
                {
                    WebClient wc = new WebClient();
                    wc.DownloadFileCompleted +=
                        Wc_DownloadFileCompleted; // Call the codes when download process complete
                    wc.DownloadProgressChanged += Wc_DownloadProgressChanged;
                    wc.DownloadFileAsync(fabric,
                        appDataDizini +
                        "/.projects/projects-fabric-" + latestFabricVersion + ".zip"); // Download fabric to directory '.projects'

                    playButtonStaticLabel.Enabled = false;
                    settingsStaticPictureBox.Enabled = false;
                    versionInfoStaticLabel.Text = "İndiriliyor...";
                    downloadCompleteLabel.Visible = true;
                    downloadCompleteBar.Visible = true;
                    playSplitStaticLabel.Visible = true;
                }
            }

            GC.WaitForPendingFinalizers();
        }

        private void Wc_DownloadProgressChanged(object sender, DownloadProgressChangedEventArgs e)
        {
            downloadCompleteLabel.Text = String.Format("{0:0.##}", Convert.ToDouble(e.BytesReceived) / 1024 / 1024) +
                                         "MB"
                                         + "/" + String.Format("{0:0.##}",
                                             Convert.ToDouble(e.TotalBytesToReceive) / 1024 / 1024) + "MB";

            downloadCompleteBar.Value = e.ProgressPercentage;
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
                Thread.Sleep(1100);
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

            GC.WaitForPendingFinalizers();
        }

        private void prepareGameToLaunch_Tick(object sender, EventArgs e)
        {
            try
            {
                if (reopenLauncher.Checked == true)
                {
                    foreach (var process in Process.GetProcessesByName("javaw"))
                    {
                        Thread.Sleep(1031);
                        playButtonStaticLabel.Enabled = false;
                        this.Visible = false;
                        Thread.Sleep(2000);
                        timer3.Start();

                        Process mcjava = Process.Start("javaw.exe");
                        mcjava.Refresh();
                        if (alreadyPlayingAnimatedLabel)
                        {
                            alreadyPlayingAnimatedLabel = false;
                        }

                        Thread.Sleep(1000);

                        prepareGameToLaunch.Stop();
                        return;
                    }
                }
                else
                {
                    foreach (var process in Process.GetProcessesByName("javaw"))
                    {
                        Thread.Sleep(1031);
                        Process mcjava = Process.Start("javaw.exe");
                        mcjava.Refresh();
                        this.Visible = false;
                        animatedPlayingLabel().GetAwaiter();
                        playButtonStaticLabel.Enabled = false;
                        if (alreadyPlayingAnimatedLabel)
                        {
                            alreadyPlayingAnimatedLabel = false;
                        }
                        while (true)
                        {
                            Thread.Sleep(15000);
                            if (mcjava.StartTime.Second < 5) continue;
                            Environment.Exit(1);
                        }
                    }
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show(
                    "Bir hata oluştu! Uygulamayı yeniden başlatmanızı tavsiye ederiz. Hatanın devamı durumunda aşağıdaki hatayı desteğe iletiniz:\n\n" +
                    ex.Message);
            }

            GC.WaitForPendingFinalizers();
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

            GC.WaitForPendingFinalizers();
        }

        private async Task onlineCountUpdater()
        {
            do
            {
                IPHostEntry proxyIP = Dns.GetHostEntry(Properties.Settings.Default.ProxyIP);

                String anyIP = "";

                foreach (IPAddress address in proxyIP.AddressList)
                {
                    anyIP = address.ToString();
                    break;
                }

                MineStat pinger = new MineStat(anyIP, 25565);
                if (pinger.ServerUp)
                {
                    serverOnlineCountStaticLabel.Text = pinger.CurrentPlayers + " kişi oynuyor!";
                }
                else
                {
                    serverOnlineCountStaticLabel.Text = "Bağlantı Yok";
                }

                await Task.Delay(5000).ConfigureAwait(false);
            } while (alreadyRelaunchWaiting == false);

            GC.WaitForPendingFinalizers();
        }

        public class debug
        {
            public string ping { get; set; }
        }

        public class Xml
        {
            public debug debug { get; set; }
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
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                   "/.projects/mods";

            if (Directory.Exists(@appDataDizini))
            {
                string myPath = @appDataDizini;
                System.Diagnostics.Process prc = new System.Diagnostics.Process();
                prc.StartInfo.FileName = myPath;
                System.Threading.Thread.Sleep(1000);
                prc.Start();
            }
            else
            {
                Directory.CreateDirectory(@appDataDizini);
                string myPath = @appDataDizini;
                System.Diagnostics.Process prc = new System.Diagnostics.Process();
                prc.StartInfo.FileName = myPath;
                System.Threading.Thread.Sleep(1000);
                prc.Start();
            }

            GC.Collect();
            GC.WaitForPendingFinalizers();
        }

        private void rpTransfer_Click(object sender, EventArgs e)
        {
            if (!Directory.Exists(@TextureDizin))
            {
                Directory.CreateDirectory(@TextureDizin);
            }

            OpenFileDialog file = new OpenFileDialog();
            file.Filter = "ZIP Dosyası |*.zip";
            file.FilterIndex = 2;
            file.RestoreDirectory = true;
            file.CheckFileExists = false;
            file.Title = "ZIP Dosyası Seçiniz.";
            file.ShowDialog();

            string DosyaYolu = file.FileName;
            string DosyaAdi = file.SafeFileName;
            System.Threading.Thread.Sleep(500);
            if (DosyaAdi != "" && DosyaYolu != "")
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

            GC.WaitForPendingFinalizers();
        }

        private void rpFolder_Click(object sender, EventArgs e)
        {
            if (Directory.Exists(@TextureDizin))
            {
                string myPath = @TextureDizin;
                System.Diagnostics.Process prc = new System.Diagnostics.Process();
                prc.StartInfo.FileName = myPath;
                System.Threading.Thread.Sleep(1000);
                prc.Start();
            }
            else
            {
                Directory.CreateDirectory(@TextureDizin);
                string myPath = @TextureDizin;
                System.Diagnostics.Process prc = new System.Diagnostics.Process();
                prc.StartInfo.FileName = myPath;
                System.Threading.Thread.Sleep(1000);
                prc.Start();
            }

            GC.Collect();
            GC.WaitForPendingFinalizers();
        }

        private void gamefolder_Click(object sender, EventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects";

            if (Directory.Exists(@appDataDizini))
            {
                string myPath = @appDataDizini;
                System.Diagnostics.Process prc = new System.Diagnostics.Process();
                prc.StartInfo.FileName = myPath;
                System.Threading.Thread.Sleep(1000);
                prc.Start();
            }
            else
            {
                Directory.CreateDirectory(@appDataDizini);
                string myPath = @appDataDizini;
                System.Diagnostics.Process prc = new System.Diagnostics.Process();
                prc.StartInfo.FileName = myPath;
                System.Threading.Thread.Sleep(1000);
                prc.Start();
            }

            GC.Collect();
            GC.WaitForPendingFinalizers();
        }

        private void timer3_Tick(object sender, EventArgs e)
        {
            if (alreadyRelaunchWaiting)
            {
                return;
            }

            alreadyRelaunchWaiting = true;

            do
            {
                Task.Delay(5000);
            } while (Process.GetProcessesByName("javaw").Any());

            if (Properties.Settings.Default.SelectedVersion != string.Empty)
            {
                versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;
            }

            playButtonStaticLabel.Enabled = true;
            this.Visible = true;
            thisTrue();
            alreadyRelaunchWaiting = false;
            onlineCountUpdater().GetAwaiter();
            prepareGameToLaunch.Stop();
            Client.Dispose();
            DiscordRpcClientSetup();
            timer3.Stop();

            GC.Collect();
            GC.WaitForPendingFinalizers();
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
            GC.Collect();
            GC.WaitForPendingFinalizers();
            Environment.Exit(0);
        }

        private void guna2ControlBox2_Click(object sender, EventArgs e)
        {
            this.WindowState = FormWindowState.Minimized;
            GC.Collect();
            GC.WaitForPendingFinalizers();
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

        private void timer1_Tick(object sender, EventArgs e)
        {
            try
            {
                //Lobi onlineCheck
                string url = "https://projectsggapi.vercel.app/api/server1";
                HttpWebRequest request = WebRequest.Create(url) as HttpWebRequest;
                string jsonverisi = "";
                using (HttpWebResponse response = request.GetResponse() as HttpWebResponse)
                {
                    StreamReader r = new StreamReader(response.GetResponseStream());
                    jsonverisi = r.ReadToEnd();
                }

                Xml xml = JsonConvert.DeserializeObject<Xml>(jsonverisi);

                if (xml.debug.ping == "false")
                {
                    lobiOnline.Image = Properties.Resources.De_Aktif;
                }
                else
                {
                    lobiOnline.Image = Properties.Resources.Aktif;
                }

                //Gaia onlineCheck
                string url2 = "https://projectsggapi.vercel.app/api/server2";
                HttpWebRequest request2 = WebRequest.Create(url2) as HttpWebRequest;
                string jsonverisi2 = "";
                using (HttpWebResponse response2 = request2.GetResponse() as HttpWebResponse)
                {
                    StreamReader r = new StreamReader(response2.GetResponseStream());
                    jsonverisi2 += r.ReadToEnd();
                }

                if (xml.debug.ping == "false")
                {
                    gaiaOnline.Image = Properties.Resources.De_Aktif;
                }
                else
                {
                    gaiaOnline.Image = Properties.Resources.Aktif;
                }
            }
            catch
            {
                lobiOnline.Text = "?";
                gaiaOnline.Text = "?";
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
        }
    }
}
