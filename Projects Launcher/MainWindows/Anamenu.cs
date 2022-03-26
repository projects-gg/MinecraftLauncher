using CmlLib.Core;
using CmlLib.Core.Auth;
using DiscordRPC;
using MCServerStatus;
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Management;
using System.Net;
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

        private string maxramlabell;
        private string minramlabell;
        private string heightlabell;
        private string widthlabell;
        private string surumlabell;
        private bool formpanell;
        private string ramInfo;

        private int widthResolution;
        private int heightResolution;
        private string heightResolutionb;
        private string heightResolutionb2;
        private string widthResolutionb;
        private string widthResolutionb2;

        private readonly string TextureDizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                            "/.projects/resourcepacks";
        private readonly string launcherdizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects";

        readonly Random rnd = new Random();

        private bool alreadyPlayingAnimatedLabel;
        private bool alreadyRelaunchWaiting;

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
                        LargeImageKey = "131231",
                        LargeImageText = "https://mc.projects.gg/",
                        SmallImageKey = "",
                    }
                });
            }
            catch
            {
                // Shouldn't happen except no internet connection or server downtime
            }
        }

        private void selectBackgroundImage()
        {
            // Grab background image
            try
            {
                var request = WebRequest.Create("https://mc.projects.gg/LauncherUpdateStream/backgrounds" + "/" + rnd.Next(10) + ".png"); // Last background image

                using (var response = request.GetResponse())
                using (var stream = response.GetResponseStream())
                    this.BackgroundImage = Bitmap.FromStream(stream);
            }
            catch
            {
                // Shouldn't happen except no internet connection or server downtime
            }
        }

        private void updateHwInfo()
        {
            // GPU
            ManagementObjectSearcher gpuSearch = new ManagementObjectSearcher("Select * From Win32_VideoController");

            foreach (ManagementObject gpuObject in gpuSearch.Get())
            {
                gpuInfo.Text = gpuObject["name"].ToString();
                break;
            }

            // RAM
            ManagementObjectSearcher ramSearch = new ManagementObjectSearcher("Select * From Win32_ComputerSystem");

            foreach (ManagementObject ramObject in ramSearch.Get())
            {
                double ramInBytes = (Convert.ToDouble(ramObject["TotalPhysicalMemory"]));
                double roundAvailableRamValueInGb = Math.Ceiling(ramInBytes / 1073741824); // <- Byte to GB conversion
                RAMInfo.Text = string.Format("{0:0.##}", Convert.ToDouble(roundAvailableRamValueInGb) * 1024) + "MB" + " = " + Convert.ToString(roundAvailableRamValueInGb) + " GB";
                break;
            }
        }

        private void Anamenu_Load(object sender, EventArgs e)
        {
            selectBackgroundImage();

            // ".projects" directory check
            if (!Directory.Exists(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/versions"))
            {
                Directory.CreateDirectory(@Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/versions");
            }

            updateHwInfo();

            DiscordRpcClientSetup();

            onlineCountUpdater().GetAwaiter();

            playerNameStaticLabel.Text = Properties.Settings.Default.NickNames;

            reopenLauncher.Checked = Properties.Settings.Default.OyunTickS;

            if (Properties.Settings.Default.SelectedVersion != string.Empty)
            {
                versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;
                versionBox.Text = Properties.Settings.Default.SelectedVersion;
            }

            if (Properties.Settings.Default.RamMax != string.Empty)
            {
                maxRamTextBox.Text = Properties.Settings.Default.RamMax;
                maxRamDynamicCalculatorLabel.Text = String.Format("{0:0.##}", Convert.ToDouble(maxRamTextBox.Text) / 1024) + "GB";
            }
            else if (maxRamDynamicCalculatorLabel.Text != "")
            {
                maxRamDynamicCalculatorLabel.Text = "";
            }

            if (Properties.Settings.Default.RamMin != string.Empty)
            {
                minRamTextBox.Text = Properties.Settings.Default.RamMin;
                try
                {
                    minRamDynamicCalculatorLabel.Text =
                        String.Format("{0:0.##}", Convert.ToDouble(minRamTextBox.Text) / 1024) + "GB";
                }
                catch
                {
                    minRamDynamicCalculatorLabel.Text = "Geçersiz Değer!";
                }
            }
            else if (maxRamDynamicCalculatorLabel.Text != "")
            {
                minRamDynamicCalculatorLabel.Text = "";
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
                var request = WebRequest.Create("https://minotar.net/body" + "/" + playerNameStaticLabel.Text);

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
        }

        private async Task Launch() // Minecraft startup settings
        {
            var path = new MinecraftPath(launcherdizin);
            var launcher = new CMLauncher(path);
            sessions = Properties.Settings.Default.NickNames;

            var ayarlar = new MLaunchOption
            {
                MinimumRamMb = int.Parse(Properties.Settings.Default.RamMin), // Get maximum ram info
                MaximumRamMb = int.Parse(Properties.Settings.Default.RamMax), // Get minimum ram info
                Session = MSession.GetOfflineSession(sessions), // Get nickname info
                ServerIp = "mc.projects.gg", // The server IP which should connected
                GameLauncherName = "Projects Minecraft",
                ScreenWidth = int.Parse(Properties.Settings.Default.ResolutionWidth), // Get width resolution info
                ScreenHeight = int.Parse(Properties.Settings.Default.ResolutionHeight), // Get height resolution info
            };
            try
            {
                var clientStartProcess =
                    await launcher.CreateProcessAsync(Properties.Settings.Default.SelectedVersion,
                        ayarlar); // Start client

                clientStartProcess.Start(); // Launch the game

                prepareGameToLaunch.Enabled = true; // Launch prepareGameToLaunch
            }
            catch (Exception ex)
            {
                NotificationAboutException(ex);
            }
        }

        private void oynabutton_Click(object sender, EventArgs e)
        {
            string surum_appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                         "/.projects/versions/fabric-loader-0.13.3-1.18.2"; // Fabric directory
            string appDataDizini =
                Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData); // AppData directory

            int MinimumRamMb = int.Parse(Properties.Settings.Default.RamMin);
            int MaximumRamMb = int.Parse(Properties.Settings.Default.RamMax);

            if (MaximumRamMb <= MinimumRamMb)
            {
                DialogResult ramExceptionResult = MessageBox.Show(
                    "Oyunu şu an başlatılamaz:\n\nVerilen azami RAM miktarı asgari\nmiktardan daha düşük.\n\nEşitleme yapılsın mı?\nTamam: Miktarları eşitle, oyunu başlat.\nİptal: Uyarıyı kapat ve oyunu başlatma.",
                    "Bilgi", MessageBoxButtons.OKCancel); //DialogResult secenek = 

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
                    "https://mc.projects.gg/LauncherUpdateStream/fabric-loader-0.13.3-1.18.2.zip"); // Fabric installer address

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
                            LargeImageKey = "131231",
                            LargeImageText = "https://mc.projects.gg/",
                            SmallImageKey = "",
                        }
                    });

                    session = MSession.GetOfflineSession(Properties.Settings.Default.NickNames); // Get nickname info

                    Thread thread = new Thread(() => Launch().GetAwaiter());
                    thread.IsBackground = true;
                    thread.Start(); // Launch the game
                    animatedPlayingLabel().GetAwaiter();
                    this.Enabled = false;
                    prepareGameToLaunch.Start(); // Launch prepareGameToLaunch
                }
                catch (Exception ex) //If fabric not exist
                {
                    DiscordRpcClientSetup();

                    prepareGameToLaunch.Stop(); // Stop prepareGameToLaunch
                    NotificationAboutException(ex);

                    this.Enabled = true; // Open components of the launcher

                    versionInfoStaticLabel.Text =
                        Properties.Settings.Default
                            .SelectedVersion; //Write version info into versionInfoStaticLabel

                    this.Enabled = true;
                }
            }
            else
            {
                DialogResult secenek = MessageBox.Show("Fabric bulunamadı! İndirmek ister misiniz?",
                    "Fabric Dosyası Eksik", MessageBoxButtons.YesNo); //Fabric dosyasının olmadığını bildir

                if (secenek == DialogResult.Yes)
                {
                    WebClient wc = new WebClient();
                    wc.DownloadFileCompleted +=
                        Wc_DownloadFileCompleted; // Call the codes when download process completed
                    wc.DownloadFileAsync(fabric,
                        appDataDizini +
                        "/.projects/fabric-loader-0.13.3-1.18.2.zip"); // Download fabric to directory '.projects'

                    this.Enabled = false;
                    versionInfoStaticLabel.Text = "İndiriliyor...";
                }
            }
        }

        private void Wc_DownloadFileCompleted(object sender, AsyncCompletedEventArgs e)
        {
            try
            {
                string zipPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                 "/.projects/fabric-loader-0.13.3-1.18.2.zip";
                string extractPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                     "/.projects/versions";

                System.IO.Compression.ZipFile.ExtractToDirectory(zipPath, extractPath);
                Thread.Sleep(1100);
                versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;
                this.Enabled = true;
            }
            catch (Exception ex)
            {
                NotificationAboutException(ex);
            }
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
                        Thread.Sleep(1000);
                        animatedPlayingLabel().GetAwaiter();
                        playButtonStaticLabel.Enabled = false;
                        this.Visible = false;
                        if (alreadyPlayingAnimatedLabel)
                        {
                            alreadyPlayingAnimatedLabel = false;
                        }
                        timer3.Stop();
                        Environment.Exit(0);
                    }
                }
            }
            catch (Exception ex)
            {
                MessageBox.Show(
                    "Bir hata oluştu! Uygulamayı yeniden başlatmanızı tavsiye ederiz. Hatanın devamı durumunda aşağıdaki hatayı desteğe iletiniz:\n\n" +
                    ex.Message);
            }
        }

        private void NotificationAboutException(Exception ex)
        {
            MessageBox.Show(
                "Başlatıcı görevi işlenirken beklenmedik bir hata oluştu.\n\nBu hata önemli olmayabilir ya da programın yanlış çalışmasına neden oluyor olabilir. Eğer sorun yaşıyorsanız uygulamayı yeniden başlatın. Hata devam ederse destek sisteminde hatayı bizimle paylaşın.\n\nHata kodu: " +
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
                try
                {
                    var proxyIP = Properties.Settings.Default.ProxyIP;
                    IMinecraftPinger pinger = new MinecraftPinger(proxyIP, 25565);
                    var status = await pinger.RequestAsync();
                    if (status != null)
                    {
                        serverOnlineCountStaticLabel.Text = status.Players.Online + " oyuncu aktif!";
                    }
                    else
                    {
                        serverOnlineCountStaticLabel.Text = "Bağlantı Yok";
                    }
                }
                catch
                {
                    serverOnlineCountStaticLabel.Text = "Sunucu Hatası";
                }

                await Task.Delay(5000).ConfigureAwait(false);
            } while (alreadyRelaunchWaiting == false);
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

        private void formpanel_Paint(object sender, PaintEventArgs e)
        {
            formpanell = settingsBgPanel.Visible;
        }

        private void previousPage_Click(object sender, EventArgs e)
        {
            settingsBgPanel.Visible = false;
            backButton.Visible = false;
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
                        maxRamDynamicCalculatorLabel.Text =
                            String.Format("{0:0.##}", Convert.ToDouble(maxRamTextBox.Text) / 1024) + "GB";
                    }
                    catch
                    {
                        maxRamDynamicCalculatorLabel.Text = "Geçersiz Değer!";
                    }
                }
                else if (maxRamDynamicCalculatorLabel.Text != "")
                {
                    maxRamDynamicCalculatorLabel.Text = "";
                }
            }
            catch
            {
                MessageBox.Show("RAM miktarı ayarlanırken bir hata meydana geldi.");
            }
        }

        private void surumsec_SelectedIndexChanged(object sender, EventArgs e)
        {
            Properties.Settings.Default.SelectedVersion = versionBox.Text;
            Properties.Settings.Default.Save();
            versionInfoStaticLabel.Text = versionBox.Text;
        }

        private void website_Click(object sender, EventArgs e)
        {
            try
            {
                System.Diagnostics.Process.Start("https://mc.projects.gg/");
            }
            catch
            {
                // Shouldn't happen except no internet connection or server downtime
            }
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

        private void instagram_Click(object sender, EventArgs e)
        {
            try
            {
                System.Diagnostics.Process.Start("https://www.instagram.com/projects.com.tr/");
            }
            catch
            {
                // Shouldn't happen except no internet connection or server downtime
            }
        }

        private Color RandomColor()
        {
            return Color.FromArgb(rnd.Next(255), rnd.Next(255), rnd.Next(255));
        }

        private void previousPage_MouseEnter(object sender, EventArgs e)
        {
            backButton.ForeColor = RandomColor();
        }

        private void previousPage_MouseLeave(object sender, EventArgs e)
        {
            backButton.ForeColor = Color.FromArgb(245, 245, 245);
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
                        minRamDynamicCalculatorLabel.Text =
                            String.Format("{0:0.##}", Convert.ToDouble(minRamTextBox.Text) / 1024) + "GB";
                    }
                    catch
                    {
                        minRamDynamicCalculatorLabel.Text = "Geçersiz Değer!";
                    }
                }
                else if (maxRamDynamicCalculatorLabel.Text != "")
                {
                    minRamDynamicCalculatorLabel.Text = "";
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

        private void modsLabel_MouseEnter(object sender, EventArgs e)
        {
            modsLabel.ForeColor = RandomColor();
        }

        private void modsLabel_MouseLeave(object sender, EventArgs e)
        {
            modsLabel.ForeColor = Color.FromArgb(245, 245, 245);
        }

        private void rpFolder_MouseEnter(object sender, EventArgs e)
        {
            rpFolder.ForeColor = RandomColor();
        }

        private void rpFolder_MouseLeave(object sender, EventArgs e)
        {
            rpFolder.ForeColor = Color.FromArgb(245, 245, 245);
        }

        private void rpTransfer_MouseEnter(object sender, EventArgs e)
        {
            transferRpLabel.ForeColor = RandomColor();
        }

        private void rpTransfer_MouseLeave(object sender, EventArgs e)
        {
            transferRpLabel.ForeColor = Color.FromArgb(245, 245, 245);
        }

        private void modsLabel_Click(object sender, EventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/mods";

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
                    MessageBox.Show(DosyaAdi + " isimli doku paketi zaten mevcut.", "", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                }
                else
                {
                    File.Copy(DosyaYolu, TextureDizin + "\\" + DosyaAdi);
                    MessageBox.Show("Doku paketi başarıyla yüklendi.");
                }
            }
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
        }

        private void gamefolder_MouseEnter(object sender, EventArgs e)
        {
            rootLabel.ForeColor = RandomColor();
        }

        private void gamefolder_MouseLeave(object sender, EventArgs e)
        {
            rootLabel.ForeColor = Color.FromArgb(245, 245, 245);
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
            this.Enabled = true;
            alreadyRelaunchWaiting = false;
            onlineCountUpdater().GetAwaiter();
            prepareGameToLaunch.Stop();
            Client.Dispose();
            DiscordRpcClientSetup();
            timer3.Stop();
        }

        private void kapattick_CheckedChanged(object sender, EventArgs e)
        {
            if (reopenLauncher.Checked)
            {
                Properties.Settings.Default.OyunTickS = true;
            }
            else
            {
                Properties.Settings.Default.OyunTickS = false;
            }

            Properties.Settings.Default.Save();
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
                    MessageBox.Show("Miktar 1024-" + ramInfo + " " + "arasında girilmeli.");
                }
                else if (Convert.ToInt32(maxRamTextBox.Text) < 1024 ||
                         Convert.ToInt32(maxRamTextBox.Text) > Convert.ToInt32(ramInfo))
                {
                    MessageBox.Show("Miktar 1024-" + ramInfo + " " + "arasında girilmeli.");
                    maxRamTextBox.Text = ramInfo;
                }
                else if (Convert.ToInt32(ramInfo) >= 1024 &&
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
                if (string.IsNullOrEmpty(minRamTextBox.Text) || Convert.ToInt32(minRamTextBox.Text) < 1024 || Convert.ToInt32(minRamTextBox.Text) > Convert.ToInt32(ramInfo))
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
                    MessageBox.Show("Çözünürlük" + " " + heightResolutionb2 + "-" + heightResolutionb + " " + "arasında girilmeli.");
                    heighttextbox.Text = heightResolutionb;
                }
                else if (Convert.ToInt32(heighttextbox.Text) < Convert.ToInt32(heightResolutionb2) || Convert.ToInt32(heighttextbox.Text) > Convert.ToInt32(heightResolutionb))
                {
                    MessageBox.Show("Çözünürlük" + " " + heightResolutionb2 + "-" + heightResolutionb + " " + "arasında girilmeli.");
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
                    MessageBox.Show("Çözünürlük" + " " + widthResolutionb2 + "-" + widthResolutionb + " " + "arasında girilmeli.");
                    widthtextbox.Text = widthResolutionb;
                }

                if (Convert.ToInt32(widthtextbox.Text) < Convert.ToInt32(widthResolutionb2) ||
                    Convert.ToInt32(widthtextbox.Text) > Convert.ToInt32(widthResolutionb))
                {
                    MessageBox.Show("Çözünürlük" + " " + widthResolutionb2 + "-" + widthResolutionb + " " + "arasında girilmeli.");
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
            discordRpcTip.SetToolTip(this.reopenLauncher, "Oyun kapatıldığında yeniden açılıp açılmayacağını seçersiniz.\n\nBu özelliğin kapalı olması durumunda oyundayken, discord oynuyor\nbilginiz oyun durumunuzda gözükmeyecektir.");
        }

        private void oynabutton_MouseEnter(object sender, EventArgs e)
        {
            playButtonStaticLabel.ForeColor = RandomColor();
        }

        private void oynabutton_MouseLeave(object sender, EventArgs e)
        {
            playButtonStaticLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
        }

        private void guna2ControlBox1_Resize(object sender, EventArgs e)
        {
            settingsBgPanel.Size = this.Size;
        }
    }
}
