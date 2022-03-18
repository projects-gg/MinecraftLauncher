using CmlLib.Core;
using CmlLib.Core.Auth;
using DiscordRPC;
using MCServerStatus;
using System;
using System.Collections.Generic;
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

        public static string sessions;
        public static MSession session;
        public static string minrambox;
        public static string maxrambox;
        public static string widthbox;
        public static string heightbox;

        public static string maxramlabell;
        public static string minramlabell;
        public static string heightlabell;
        public static string widthlabell;
        public static string surumlabell;
        public static bool formpanell;
        public static string ramInfo;

        public static int widthResolution;
        public static int heightResolution;
        private string heightResolutionb;
        private string heightResolutionb2;
        private string widthResolutionb;
        private string widthResolutionb2;

        public static string TextureDizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                            "/.projects/resourcepacks";
        string launcherdizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects";
        
        Random rnd = new Random();

        int colorX, colorY, colorZ;

        public bool alreadyPlayingAnimatedLabel = false;
        public bool alreadyRelaunchWaiting = false;
        
        public DiscordRpcClient Client { get; private set; }

        public void DiscordRpcClientSetup()
        {
            try
            {
                Client = new DiscordRpcClient("949311557542756362");
                Client.Initialize();

                Client.SetPresence(new RichPresence()
                {
                    Details = "Başlatıcı menüsünde",
                    State = "Sunucu IP: mc.projects.gg",
                    Assets = new Assets()
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

        public void selectBackgroundImage()
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

        public void updateHwInfo()
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
                RAMInfo.Text = string.Format("{0:0.##}", Convert.ToDouble(roundAvailableRamValueInGb) * 1024) + "MB" + " = " + roundAvailableRamValueInGb.ToString() + " GB";
                break;
            }
        }

        private void Anamenu_Load(object sender, EventArgs e)
        {
            selectBackgroundImage();

            // ".projects" directory check
            if (!Directory.Exists(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/versions"))
                Directory.CreateDirectory(@Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/versions");

            updateHwInfo();

            DiscordRpcClientSetup();

            onlineCountUpdater();

            playerNameStaticLabel.Text = Properties.Settings.Default.NickNames;

            reopenLauncherCheckBox.Checked = Properties.Settings.Default.OyunTickS;
            
            if (Properties.Settings.Default.SelectedVersion != string.Empty)
            {
                versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;
                versionSelectComboBox.Text = Properties.Settings.Default.SelectedVersion;
            }
            
            if (Properties.Settings.Default.RamMax != string.Empty) {
                maxRamTextBox.Text = Properties.Settings.Default.RamMax;
                maxRamDynamicCalculatorLabel.Text = String.Format("{0:0.##}", Convert.ToDouble(maxRamTextBox.Text) / 1024) + "GB";
            }
            else if (maxRamDynamicCalculatorLabel.Text != "")
                maxRamDynamicCalculatorLabel.Text = "";

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
                minRamDynamicCalculatorLabel.Text = "";
            
            minRamTextBox.MaxLength = 4;

            // Grab resolution data
            if (Properties.Settings.Default.ResolutionHeight != string.Empty)
                widthtextbox.Text = Properties.Settings.Default.ResolutionHeight;
            else if (Properties.Settings.Default.ResolutionWidth != string.Empty)
                heighttextbox.Text = Properties.Settings.Default.ResolutionWidth;

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

        public void path() //Launcher Dizin Ayarları - Connection Limit
        {
            System.Net.ServicePointManager.DefaultConnectionLimit = 256;
            var path = new MinecraftPath(launcherdizin);
            var launcher = new CMLauncher(path);
        }

        public async void Launch() // Minecraft startup settings
        {
            var path = new MinecraftPath(launcherdizin);
            var launcher = new CMLauncher(path);
            sessions = loginMenuForm.nickname;

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
            var clientStartProcess =
                await launcher.CreateProcessAsync(Properties.Settings.Default.SelectedVersion, ayarlar); // Start client

            clientStartProcess.Start(); // Launch the game

            prepareGameToLaunch.Enabled = true; // Launch prepareGameToLaunch
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
                    MaximumRamMb = MinimumRamMb;
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

                    Client.SetPresence(new RichPresence()
                    {
                        Details = "Şu an oyunda!",
                        State = "Sunucu IP: mc.projects.gg",
                        Timestamps = new Timestamps()
                        {
                            Start = DateTime.UtcNow
                        },
                        Assets = new Assets()
                        {
                            LargeImageKey = "131231",
                            LargeImageText = "https://mc.projects.gg/",
                            SmallImageKey = "",
                        }
                    });

                    session = MSession.GetOfflineSession(loginMenuForm.nickname); // Get nickname info

                    Thread thread = new Thread(() => Launch());
                    thread.IsBackground = true;
                    thread.Start(); // Launch the game

                    animatedPlayingLabel();
                    this.Enabled = false;
                    prepareGameToLaunch.Start(); // Launch prepareGameToLaunch
                }
                catch //If fabric not exist
                {
                    DiscordRpcClientSetup();

                    prepareGameToLaunch.Stop(); // Stop prepareGameToLaunch
                    MessageBox.Show("Oyunu başlatırken bir sorun meydana geldi.", "Bilgi",
                        MessageBoxButtons.OK); //DialogResult secenek = 

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
                if (reopenLauncherCheckBox.Checked == true)
                {
                    foreach (var process in Process.GetProcessesByName("javaw"))
                    {
                        Thread.Sleep(1031);
                        animatedPlayingLabel();
                        playButtonStaticLabel.Enabled = false;
                        this.Visible = false;
                        Thread.Sleep(2000);
                        timer3.Start();

                        Process mcjava = Process.Start("javaw.exe");
                        mcjava.Refresh();
                        if (alreadyPlayingAnimatedLabel)
                            alreadyPlayingAnimatedLabel = false;
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
                        animatedPlayingLabel();
                        playButtonStaticLabel.Enabled = false;
                        this.Visible = false;
                        if (alreadyPlayingAnimatedLabel)
                            alreadyPlayingAnimatedLabel = false;
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

        private async void animatedPlayingLabel()
        {
            if (alreadyPlayingAnimatedLabel)
                return;

            alreadyPlayingAnimatedLabel = true;
            versionInfoStaticLabel.Text = "Başlatılıyor";

            do
            {
                await Task.Delay(250);

                if (versionInfoStaticLabel.Text.Equals("Başlatılıyor"))
                    versionInfoStaticLabel.Text = "Başlatılıyor.";
                else if (versionInfoStaticLabel.Text.Equals("Başlatılıyor."))
                    versionInfoStaticLabel.Text = "Başlatılıyor..";
                else if (versionInfoStaticLabel.Text.Equals("Başlatılıyor.."))
                    versionInfoStaticLabel.Text = "Başlatılıyor...";
                else if (versionInfoStaticLabel.Text.Equals("Başlatılıyor..."))
                    versionInfoStaticLabel.Text = "Başlatılıyor";

                await Task.Delay(250);
            } while (alreadyPlayingAnimatedLabel);
        }

        private void ayarlarbutton_Click(object sender, EventArgs e)
        {
            if (settingsBgPanel.Visible == false)
            {
                previousPageStaticLabel.Visible = true;
                settingsBgPanel.Visible = true;
            }
            else
                settingsBgPanel.Visible = false;
        }

        private async void onlineCountUpdater()
        {
            do
            {
                try
                {
                    /* I can't figure out how to resolve A record to IPv4 address.
                    IPHostEntry hostEntrcolorY = Dns.GetHostEntry("mc.projects.gg");
    
                    string hostIpString = Convert.ToString(hostEntry.AddressList[0].MapToIPv4());
                    */

                    IMinecraftPinger pinger = new MinecraftPinger("193.164.7.43", 25565);
                    var status = await pinger.RequestAsync();
                    String server = status.Players.Online + "";
                    serverOnlineCountStaticLabel.Text = (server + " oyuncu aktif!");
                }
                catch
                {
                    serverOnlineCountStaticLabel.Text = ("Sunucu Hatası");
                }

                await Task.Delay(5000);
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

        private void geriformpanel_Click(object sender, EventArgs e)
        {
            settingsBgPanel.Visible = false;
            previousPageStaticLabel.Visible = false;
        }

        private void widthtextbox_TextChanged(object sender, EventArgs e)
        {
            widthbox = widthtextbox.Text;
            Properties.Settings.Default.ResolutionWidth = widthbox;
            Properties.Settings.Default.Save();
            Projects_Launcher.mainMenuForm.widthlabell = Properties.Settings.Default.ResolutionWidth;
        }

        private void heighttextbox_TextChanged(object sender, EventArgs e)
        {
            heightbox = heighttextbox.Text;
            Properties.Settings.Default.ResolutionHeight = heightbox;
            Properties.Settings.Default.Save();
            Projects_Launcher.mainMenuForm.heightlabell = Properties.Settings.Default.ResolutionHeight;
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
            Properties.Settings.Default.SelectedVersion = versionSelectComboBox.Text;
            Properties.Settings.Default.Save();
            versionInfoStaticLabel.Text = versionSelectComboBox.Text;
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

        private void geriformpanel_MouseEnter(object sender, EventArgs e)
        {
            colorX = rnd.Next(255);
            colorY = rnd.Next(255);
            colorZ = rnd.Next(255);
            previousPageStaticLabel.ForeColor = System.Drawing.Color.FromArgb(colorX, colorY, colorZ);
        }

        private void geriformpanel_MouseLeave(object sender, EventArgs e)
        {
            previousPageStaticLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
        }

        private void changelogs_MouseEnter(object sender, EventArgs e)
        {
            colorX = rnd.Next(255);
            colorY = rnd.Next(255);
            colorZ = rnd.Next(255);
            previousPageStaticLabel.ForeColor = System.Drawing.Color.FromArgb(colorX, colorY, colorZ);
        }

        private void changelogs_MouseLeave(object sender, EventArgs e)
        {
            playButtonStaticLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
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
                minramlabell = minramlabel.Text;
        }

        private void mods_MouseEnter(object sender, EventArgs e)
        {
            colorX = rnd.Next(255);
            colorY = rnd.Next(255);
            colorZ = rnd.Next(255);
            modsDirStaticLabel.ForeColor = System.Drawing.Color.FromArgb(colorX, colorY, colorZ);
        }

        private void mods_MouseLeave(object sender, EventArgs e)
        {
            modsDirStaticLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
        }

        private void texturepackfolder_MouseEnter(object sender, EventArgs e)
        {
            colorX = rnd.Next(255);
            colorY = rnd.Next(255);
            colorZ = rnd.Next(255);
            resourcePackDirLabel.ForeColor = System.Drawing.Color.FromArgb(colorX, colorY, colorZ);
        }

        private void texturepackfolder_MouseLeave(object sender, EventArgs e)
        {
            resourcePackDirLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
        }

        private void texturepackaktar_MouseEnter(object sender, EventArgs e)
        {
            colorX = rnd.Next(255);
            colorY = rnd.Next(255);
            colorZ = rnd.Next(255);
            transferResourcepackLabel.ForeColor = System.Drawing.Color.FromArgb(colorX, colorY, colorZ);
        }

        private void texturepackaktar_MouseLeave(object sender, EventArgs e)
        {
            transferResourcepackLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
        }

        private void mods_Click(object sender, EventArgs e)
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

        private void texturepackaktar_Click(object sender, EventArgs e)
        {
            if (!Directory.Exists(@TextureDizin))
                Directory.CreateDirectory(@TextureDizin);

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
                    MessageBox.Show(DosyaAdi + " isimli doku paketi zaten mevcut.", "", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                else
                {
                    File.Copy(DosyaYolu, TextureDizin + "\\" + DosyaAdi);
                    MessageBox.Show("Doku paketi başarıyla yüklendi.");
                }
            }
        }

        private void texturepackfolder_Click(object sender, EventArgs e)
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
            colorX = rnd.Next(255);
            colorY = rnd.Next(255);
            colorZ = rnd.Next(255);
            gameDirStaticLabel.ForeColor = System.Drawing.Color.FromArgb(colorX, colorY, colorZ);
        }

        private void gamefolder_MouseLeave(object sender, EventArgs e)
        {
            gameDirStaticLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
        }

        private async void timer3_Tick(object sender, EventArgs e)
        {
            if (alreadyRelaunchWaiting)
                return;

            alreadyRelaunchWaiting = true;

            do
            {
                await Task.Delay(5000);
            } while (Process.GetProcessesByName("javaw").Any());

            if (Properties.Settings.Default.SelectedVersion != string.Empty)
                versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;

            playButtonStaticLabel.Enabled = true;
            this.Visible = true;
            this.Enabled = true;
            alreadyRelaunchWaiting = false;
            onlineCountUpdater();
            prepareGameToLaunch.Stop();
            Client.Dispose();
            DiscordRpcClientSetup();
            timer3.Stop();
        }

        private void kapattick_CheckedChanged(object sender, EventArgs e)
        {
            if (reopenLauncherCheckBox.Checked)
                Properties.Settings.Default.OyunTickS = true;
            else
                Properties.Settings.Default.OyunTickS = false;

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
                    MessageBox.Show("Miktar 1024-" + ramInfo + " " + "arasında girilmeli.");
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
                if (string.IsNullOrEmpty(minRamTextBox.Text))
                {
                    MessageBox.Show("Miktar 1024-" + ramInfo + " " + "arasında girilmeli.");
                    minRamTextBox.Text = ramInfo;
                }
                else if (Convert.ToInt32(minRamTextBox.Text) < 1024 || Convert.ToInt32(minRamTextBox.Text) > Convert.ToInt32(ramInfo))
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
            discordRpcTip.SetToolTip(this.reopenLauncherCheckBox, "Oyun kapatıldığında yeniden açılıp açılmayacağını seçersiniz.\n\nBu özelliğin kapalı olması durumunda oyundayken, discord oynuyor\nbilginiz oyun durumunuzda gözükmeyecektir.");
        }

        private void oynabutton_MouseEnter(object sender, EventArgs e)
        {
            colorX = rnd.Next(255);
            colorY = rnd.Next(255);
            colorZ = rnd.Next(255);
            playButtonStaticLabel.ForeColor = System.Drawing.Color.FromArgb(colorX, colorY, colorZ);
        }

        private void oynabutton_MouseLeave(object sender, EventArgs e)
        {
            playButtonStaticLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
        }
    }
}
