﻿using CmlLib.Core;
using CmlLib.Core.Auth;
using DiscordRPC;
using ICSharpCode.SharpZipLib.Zip;
using MCServerStatus;
using Microsoft.Win32;
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Text;
using System.IO;
using System.Linq;
using System.Management;
using System.Net;
using System.Net.NetworkInformation;
using System.Threading;
using System.Threading.Tasks;
using System.IO.Compression;
using System.Security.Policy;
using System.Windows.Forms;

namespace Projects_Launcher.Projects_Launcher
{
    public partial class mainMenuForm : Form
    {
        public mainMenuForm()
        {
            

            InitializeComponent();
        }

        public static string versiyon;

        public static string sessions;
        public static MSession session;
        public static int index;
        public static string rammiktar;
        public static string height;
        public static string width;
        public static string versiyons;
        public static string minrambox;
        public static string maxrambox;
        public static string widthbox;
        public static string heightbox;
        public static string sayac;

        public static string maxramlabell;
        public static string minramlabell;
        public static string heightlabell;
        public static string widthlabell;
        public static string surumlabell;
        public static bool formpanell;

        public static string rambilgi;

        public static int genislik;
        public static int yukseklik;
        private string yukseklikb;
        private string yukseklikb2;
        private string genislikb;
        private string genislikb2;
        Ping p = new Ping();

        public static string TextureDizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/resourcepacks";
        string launcherdizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects";

        Uri fabric = new Uri("https://mc.projects.gg/LauncherUpdateStream/fabric-loader-0.13.3-1.18.2.zip");

        string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);

        Random rnd = new Random();
        int x, y, z;

        public bool alreadyPlayingAnimatedLabel = false;

        private int uiThreadId = Thread.CurrentThread.ManagedThreadId;
        public DiscordRpcClient Client { get; private set; }
        public void Setup()
        {

            //Client.Dispose();
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

        private void Anamenu_Load(object sender, EventArgs e)
        {
            var random = new Random();
            var BackgroundList = new List<string> { "kıs_meydan.png","balık2.png","kıs_meydan2.png", "maden.png", "maden2.png", "meydan.png", "world.png", "world2.png", "world3.png", "world4.png" };
            index = random.Next(BackgroundList.Count);

            //projects kontrol
            if (Directory.Exists(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/versions"))
            {

            }
            else
            {
                Directory.CreateDirectory(@Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/versions");
            }

            //Donanım Bilgileri
            try
            {
                //Ekran Kartı
                ManagementObjectSearcher ekran = new ManagementObjectSearcher("Select * From Win32_VideoController");

                foreach (ManagementObject Mobject in ekran.Get())
                {
                    gpuInfo.Text = Mobject["name"].ToString();
                }

                //RAM
                ManagementObjectSearcher Search = new ManagementObjectSearcher("Select * From Win32_ComputerSystem");

                foreach (ManagementObject Mobject in Search.Get())
                {
                    double Ram_Bytes = (Convert.ToDouble(Mobject["TotalPhysicalMemory"]));
                    double ramgb = Ram_Bytes / 1073741824;
                    double islem = Math.Ceiling(ramgb);
                    RAMInfo.Text = String.Format("{0:0.##}", Convert.ToDouble(islem) * 1024) + "MB"  + " = " + islem.ToString() + " GB";
                }
            }
            catch
            {

            }
            Setup(); //Discord Oynuyor
            
            timer2.Start(); //Ping Sayaç

            playerNameStaticLabel.Text = Properties.Settings.Default.NickNames; //Nickname Bilgisini Göster

            //Oyun Kapanınca Aç / Tick
            try
            {
                if (Properties.Settings.Default.OyunTickS != string.Empty)
                {
                    ticksave.Text = Properties.Settings.Default.OyunTickS;
                }
                if (ticksave.Text == "acik")
                {
                    reopenLauncherCheckBox.Checked = true;
                }
                if (ticksave.Text == "kapali")
                {
                    reopenLauncherCheckBox.Checked = false;
                }
            }
            catch
            {

            }

            //Versiyon bilgisini al
            try
            {
                if (Properties.Settings.Default.SelectedVersion != string.Empty)
                {
                    versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;
                }
            }
            catch
            {

            }

            //Versiyon bilgisini al / II
            try
            {
                if (Properties.Settings.Default.SelectedVersion != string.Empty)
                {
                    versionSelectComboBox.Text = Properties.Settings.Default.SelectedVersion;
                }
            }
            catch
            {

            }

            //Ram bilgisini al
            try
            {
                

                if (Properties.Settings.Default.RamMax != string.Empty)
                {
                    maxRamTextBox.Text = Properties.Settings.Default.RamMax;
                }


                if (Properties.Settings.Default.RamMax != string.Empty)
                {
                    maxramlabel.Text = Properties.Settings.Default.RamMax;
                    try
                    {
                        maxRamDynamicCalculatorLabel.Text = String.Format("{0:0.##}", Convert.ToDouble(maxRamTextBox.Text) / 1024) + "GB";
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
               

                


                //min
                if (Properties.Settings.Default.RamMin != string.Empty)
                {
                    minRamTextBox.Text = Properties.Settings.Default.RamMin;
                }
                if (Properties.Settings.Default.RamMin != string.Empty)
                {
                    try
                    {
                        minRamDynamicCalculatorLabel.Text = String.Format("{0:0.##}", Convert.ToDouble(minRamTextBox.Text) / 1024) + "GB";
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
            }
            catch
            {

            }

            //Resolution bilgisini al
            try
            {
                if (Properties.Settings.Default.ResolutionHeight != string.Empty)
                {
                    widthtextbox.Text = Properties.Settings.Default.ResolutionHeight;
                }
                if (Properties.Settings.Default.ResolutionWidth != string.Empty)
                {
                    heighttextbox.Text = Properties.Settings.Default.ResolutionWidth;
                }
            }
            catch
            {

            }


            //Skin bilgisini al
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

            }

            //Arkaplan bilgisini al
            try
            {
                var request = WebRequest.Create("https://mc.projects.gg/LauncherUpdateStream/background" + "/" + (BackgroundList[index]));

                using (var response = request.GetResponse())
                using (var stream = response.GetResponseStream())
                {
                    this.BackgroundImage = Bitmap.FromStream(stream);
                   
                }
            }
            catch
            {

            }
        }
        public void path() //Launcher Dizin Ayarları - Connection Limit
        {
            System.Net.ServicePointManager.DefaultConnectionLimit = 256;

            var path = new MinecraftPath(launcherdizin);
            //var path = new MinecraftPath();

            var launcher = new CMLauncher(path);

            /*foreach (var item in launcher.GetAllVersions())
            {
                versiyonselect.Items.Add(item.Name);
            }*/

            try
            {

            }
            catch
            {

            }
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
            var clientStartProcess = await launcher.CreateProcessAsync(Properties.Settings.Default.SelectedVersion, ayarlar); // Start client

            clientStartProcess.Start(); // Launch the game

            timer1.Enabled = true; // Launch timer1
            
            
        }


        private void oynabutton_Click(object sender, EventArgs e)
        {
            string surum_appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/versions/fabric-loader-0.13.3-1.18.2"; // Fabric directory
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData); // AppData directory

            int MinimumRamMb = int.Parse(Properties.Settings.Default.RamMin);
            int MaximumRamMb = int.Parse(Properties.Settings.Default.RamMax);

            if (MaximumRamMb <= MinimumRamMb)
            {
                DialogResult ramExceptionResult = MessageBox.Show("Oyunu şu an başlatılamaz:\n\nVerilen azami RAM miktarı asgari\nmiktardan daha düşük.\n\nEşitleme yapılsın mı?\nTamam: Miktarları eşitle, oyunu başlat.\nİptal: Uyarıyı kapat ve oyunu başlatma.", "Bilgi", MessageBoxButtons.OKCancel); //DialogResult secenek = 

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

            Uri fabric = new Uri("https://mc.projects.gg/LauncherUpdateStream/fabric-loader-0.13.3-1.18.2.zip"); // Fabric installer address
            try
            {
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
                        timer1.Start(); // Launch timer1

                    }
                    catch //If fabric not exist
                    {
                        Client.Dispose();
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

                        timer1.Stop(); //Stop timer1
                        MessageBox.Show("Oyunu başlatırken bir sorun meydana geldi.", "Bilgi", MessageBoxButtons.OK); //DialogResult secenek = 

                        this.Enabled = true; //Launcherın bileşenlerini aktifleştir

                        /* Unnecessary boilerplate
                        if (Properties.Settings.Default.SelectedVersion != string.Empty)
                        {
                            versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion; // Write version info into versionInfoStaticLabel
                        }
                        */

                        versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion; //Write version info into versionInfoStaticLabel

                        this.Enabled = true;

                    }


                }
                else
                {

                    DialogResult secenek = MessageBox.Show("Bazı Dosyalar Bulunamadı! İndirmek ister misiniz?", "Projects Launcher", MessageBoxButtons.YesNo); //Fabric dosyasının olmadığını bildir

                    if (secenek == DialogResult.Yes)
                    {
                        WebClient wc = new WebClient();
                        wc.DownloadFileCompleted += Wc_DownloadFileCompleted; // Call the codes when download process completed
                        wc.DownloadFileAsync(fabric, appDataDizini + "/.projects/fabric-loader-0.13.3-1.18.2.zip"); // Download fabric to directory '.projects'

                        this.Enabled = false;
                        versionInfoStaticLabel.Text = "İndiriliyor...";
                    }
                }
            }
            catch
            {

            }
          
        }
        private void Wc_DownloadFileCompleted(object sender, AsyncCompletedEventArgs e)
        {
            try
            {
                string zipPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/fabric-loader-0.13.3-1.18.2.zip";
                string extractPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/versions";

                System.IO.Compression.ZipFile.ExtractToDirectory(zipPath, extractPath);
                Thread.Sleep(1100);
                versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;
                this.Enabled = true;
            }
            catch
            {

            }
            
        }

        private void timer1_Tick(object sender, EventArgs e)
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
                        timer1.Stop();
                        break;
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
                MessageBox.Show("Bir hata oluştu! Uygulamayı yeniden başlatmanızı tavsiye ederiz. Hatanın devamı durumunda aşağıdaki hatayı desteğe iletiniz:\n\n" + ex.Message);
            }
          
        }

        private async void animatedPlayingLabel()
        {
            if (alreadyPlayingAnimatedLabel)
                return;

            alreadyPlayingAnimatedLabel = true;

            do
            {
                await Task.Delay(250);

                if (versionInfoStaticLabel.Text.Equals("Başlatılıyor"))
                    versionInfoStaticLabel.Text = "Başlatılıyor.";
                else if (versionInfoStaticLabel.Text.Equals("Başlatılıyor."))
                    versionInfoStaticLabel.Text = "Başlatılıyor..";
                else if (versionInfoStaticLabel.Text.Equals("Başlatılıyor.."))
                    versionInfoStaticLabel.Text = "Başlatılıyor...";
                else
                    versionInfoStaticLabel.Text = "Başlatılıyor";

                await Task.Delay(250);
            } while (alreadyPlayingAnimatedLabel);
        }

        private void ayarlarbutton_Click(object sender, EventArgs e)
        {
            try
            {
                if (settingsBgPanel.Visible == false)
                {
                    previousPageStaticLabel.Visible = true;
                    settingsBgPanel.Visible = true;
                }
                else
                {
                    settingsBgPanel.Visible = false;
                }
            }
            catch
            {

            }
           


        }

        private async Task ServerStatus()
        {
            try
            {
                /* I can't figure out how to resolve A record to IPv4 address.
                IPHostEntry hostEntry = Dns.GetHostEntry("mc.projects.gg");

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

        }

        public virtual long Speed { get; }
        private async void timer2_Tick(object sender, EventArgs e)
        {

            try
            {
                //ping
                //pingsayac++;

                //string a, b, c;
                //PingReply pr = p.Send("mc.projects.gg");
                //a = pr.Status.ToString();
                //b = pr.Address.ToString();
                //c = pr.RoundtripTime.ToString();
                //pingsayacc.Text = string.Format("{2} ms", a, b, c);

                //player
                await ServerStatus();

            }
            catch
            {
                // Shouldn't happen except no internet connection or server downtime
            }
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
                        maxRamDynamicCalculatorLabel.Text = String.Format("{0:0.##}", Convert.ToDouble(maxRamTextBox.Text) / 1024) + "GB";
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
           
            try
            {
                Properties.Settings.Default.SelectedVersion = versionSelectComboBox.Text;
                Properties.Settings.Default.Save();
                versionInfoStaticLabel.Text = versionSelectComboBox.Text;
            }
            catch
            {

            }
        }

        private void website_Click(object sender, EventArgs e)
        {
            try
            {
                System.Diagnostics.Process.Start("https://mc.projects.gg/");
            }
            catch
            {

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

            }
        }

        private void geriformpanel_MouseEnter(object sender, EventArgs e)
        {
            try
            {
                x = rnd.Next(255);
                y = rnd.Next(255);
                z = rnd.Next(255);
                previousPageStaticLabel.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
            }
            catch
            {

            }
        }

        private void geriformpanel_MouseLeave(object sender, EventArgs e)
        {
            
            try
            {
                previousPageStaticLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
            }
            catch
            {

            }
        }

        private void changelogs_MouseEnter(object sender, EventArgs e)
        {
           
            try
            {
                x = rnd.Next(255);
                y = rnd.Next(255);
                z = rnd.Next(255);
                previousPageStaticLabel.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
            }
            catch
            {

            }
        }

        private void changelogs_MouseLeave(object sender, EventArgs e)
        {
            
            try
            {
                playButtonStaticLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
            }
            catch
            {

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
                        minRamDynamicCalculatorLabel.Text = String.Format("{0:0.##}", Convert.ToDouble(minRamTextBox.Text) / 1024) + "GB";
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
            try
            {
                minramlabell = minramlabel.Text;
            }
            catch
            {

            }
           
        }

        private void mods_MouseEnter(object sender, EventArgs e)
        {
            
            try
            {
                x = rnd.Next(255);
                y = rnd.Next(255);
                z = rnd.Next(255);
                modsDirStaticLabel.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
            }
            catch
            {

            }
        }

        private void mods_MouseLeave(object sender, EventArgs e)
        {
            
            try
            {
                modsDirStaticLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
            }
            catch
            {

            }
        }

        private void texturepackfolder_MouseEnter(object sender, EventArgs e)
        {
           
            try
            {
                x = rnd.Next(255);
                y = rnd.Next(255);
                z = rnd.Next(255);
                resourcePackDirLabel.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
            }
            catch
            {

            }
        }

        private void texturepackfolder_MouseLeave(object sender, EventArgs e)
        {
            
            try
            {
                resourcePackDirLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
            }
            catch
            {

            }
        }

        private void texturepackaktar_MouseEnter(object sender, EventArgs e)
        {
          
            try
            {
                x = rnd.Next(255);
                y = rnd.Next(255);
                z = rnd.Next(255);
                transferResourcepackLabel.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
            }
            catch
            {

            }
        }

        private void texturepackaktar_MouseLeave(object sender, EventArgs e)
        {
            
            try
            {
                transferResourcepackLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
            }
            catch
            {

            }
        }

        private void mods_Click(object sender, EventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/mods";


            try
            {
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
            catch
            {

            }
        }

        private void texturepackaktar_Click(object sender, EventArgs e)
        {
           

            try
            {
                if (Directory.Exists(@TextureDizin))
                {
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
                else
                {
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
            }
            catch
            {

            }
        }

        private void texturepackfolder_Click(object sender, EventArgs e)
        {
            

            try
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
            catch
            {

            }
        }

        private void gamefolder_Click(object sender, EventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects";



            try
            {
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
            catch
            {

            }
        }

        private void gamefolder_MouseEnter(object sender, EventArgs e)
        {
          
            try
            {
                x = rnd.Next(255);
                y = rnd.Next(255);
                z = rnd.Next(255);
                gameDirStaticLabel.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
            }
            catch
            {

            }
        }

        private void gamefolder_MouseLeave(object sender, EventArgs e)
        {
            
            try
            {
                gameDirStaticLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
            }
            catch
            {

            }
        }

        private void timer3_Tick(object sender, EventArgs e)
        {

            try
            {
                if (!Process.GetProcessesByName("javaw").Any())
                {
                    if (Properties.Settings.Default.SelectedVersion != string.Empty)
                    {
                        versionInfoStaticLabel.Text = Properties.Settings.Default.SelectedVersion;
                    }
                    playButtonStaticLabel.Enabled = true;
                    this.Visible = true;
                    this.Enabled = true;
                    timer1.Stop();

                    Client.Dispose();
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
                    timer3.Stop();
                }
                else
                {
                    timer1.Start();
                }
            }
            catch
            {

            }
        }

        private void kapattick_CheckedChanged(object sender, EventArgs e)
        {

          
            try
            {
                if (reopenLauncherCheckBox.Checked == true)
                {
                    ticksave.Text = "acik";

                    Properties.Settings.Default.OyunTickS = ticksave.Text;
                    Properties.Settings.Default.Save();
                }
                else
                {
                    ticksave.Text = "kapali";

                    Properties.Settings.Default.OyunTickS = ticksave.Text;
                    Properties.Settings.Default.Save();
                }
            }
            catch
            {

            }
        }

        private void maxramtext_Leave(object sender, EventArgs e)
        {
            try
            {
                ManagementObjectSearcher Search = new ManagementObjectSearcher("Select * From Win32_ComputerSystem");
                foreach (ManagementObject Mobject in Search.Get())
                {

                    double Ram_Bytes = (Convert.ToDouble(Mobject["TotalPhysicalMemory"]));
                    double ramgb = Ram_Bytes / 1073741824;
                    double islem = Math.Ceiling(ramgb);
                    rambilgi = String.Format("{0:0.##}", Convert.ToDouble(islem) * 1024 - 1024);
                }

                maxRamTextBox.Text = (maxRamTextBox.Text).Trim();
                if (string.IsNullOrEmpty(maxRamTextBox.Text))
                    MessageBox.Show("Miktar 1024-" + rambilgi + " " + "arasında girilmeli.");
                else if (Convert.ToInt32(maxRamTextBox.Text) < 1024 || Convert.ToInt32(maxRamTextBox.Text) > Convert.ToInt32(rambilgi))
                {
                    MessageBox.Show("Miktar 1024-" + rambilgi + " " + "arasında girilmeli.");
                    maxRamTextBox.Text = rambilgi;
                } else if (Convert.ToInt32(rambilgi) >= 1024 && Convert.ToInt32(maxRamTextBox.Text) > Convert.ToInt32(rambilgi)-512)
                {
                    MessageBox.Show("Yüksek kaynak kullanımı!\n\nYüksek kaynak tüketimi bilgisayarınızdaki\nbazı şeylerin yavaş çalışmasına neden olabilir\nEn yüksek RAM miktarınızı, azami RAM\nmiktarından daha az tutmanız tavsiye\nedilir.", "Kaynak Tüketim Uyarısı");
                    maxRamTextBox.Text = rambilgi;
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
                    rambilgi = String.Format("{0:0.##}", Convert.ToDouble(islem) * 512);
                }

                minRamTextBox.Text = (minRamTextBox.Text).Trim();
                if (!string.IsNullOrEmpty(minRamTextBox.Text))
                {

                }
                else
                {
                    MessageBox.Show("Miktar 1024-" + rambilgi + " " + "arasında girilmeli.");
                    minRamTextBox.Text = rambilgi;
                }

                if (Convert.ToInt32(minRamTextBox.Text) < 1024 || Convert.ToInt32(minRamTextBox.Text) > Convert.ToInt32(rambilgi))
                {
                    MessageBox.Show("Miktar 1024-" + rambilgi + " " + "arasında girilmeli.");
                    minRamTextBox.Text = rambilgi;
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
                yukseklik = Screen.PrimaryScreen.Bounds.Height;
                yukseklikb = String.Format("{0:0.##}", Convert.ToDouble(yukseklik) / 2);
                yukseklikb2 = String.Format("{0:0.##}", Convert.ToDouble(yukseklikb) / 2);

                heighttextbox.Text = (heighttextbox.Text).Trim();
                if (!string.IsNullOrEmpty(heighttextbox.Text))
                {

                }
                else
                {
                    MessageBox.Show("Çözünürlük" + " " + yukseklikb2 + "-" + yukseklikb + " " + "arasında girilmeli.");
                    heighttextbox.Text = yukseklikb;
                }

                if (Convert.ToInt32(heighttextbox.Text) < Convert.ToInt32(yukseklikb2) || Convert.ToInt32(heighttextbox.Text) > Convert.ToInt32(yukseklikb))
                {
                    MessageBox.Show("Çözünürlük" + " " + yukseklikb2 + "-" + yukseklikb + " " + "arasında girilmeli.");
                    heighttextbox.Text = yukseklikb2;
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
                genislik = Screen.PrimaryScreen.Bounds.Width;
                genislikb = String.Format("{0:0.##}", Convert.ToDouble(genislik) / 2);
                genislikb2 = String.Format("{0:0.##}", Convert.ToDouble(genislikb) / 2);

                widthtextbox.Text = (widthtextbox.Text).Trim();
                if (!string.IsNullOrEmpty(widthtextbox.Text))
                {

                }
                else
                {
                    MessageBox.Show("Çözünürlük" + " " + genislikb2 + "-" + genislikb + " " + "arasında girilmeli.");
                    widthtextbox.Text = genislikb;
                }

                if (Convert.ToInt32(widthtextbox.Text) < Convert.ToInt32(genislikb2) || Convert.ToInt32(widthtextbox.Text) > Convert.ToInt32(genislikb))
                {
                    MessageBox.Show("Çözünürlük" + " " + genislikb2 + "-" + genislikb + " " + "arasında girilmeli.");
                    widthtextbox.Text = genislikb;
                }
            }
            catch
            {
                MessageBox.Show("Çözünürlük ayarlanırken bir hata meydana geldi.");
            }
            
        }

        private void guna2ControlBox3_Click(object sender, EventArgs e)
        {
            try
            {
                Environment.Exit(0);
            }
            catch
            {

            }
        }

        private void guna2ControlBox2_Click(object sender, EventArgs e)
        {  
            try
            {
                this.WindowState = FormWindowState.Minimized;
            }
            catch
            {

            }
        }

        private void oynabutton_MouseEnter(object sender, EventArgs e)
        {
            try
            {
                x = rnd.Next(255);
                y = rnd.Next(255);
                z = rnd.Next(255);
                playButtonStaticLabel.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
            }
            catch
            {

            }
        }

        private void oynabutton_MouseLeave(object sender, EventArgs e)
        {
            try
            {
                playButtonStaticLabel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
            }
            catch
            {

            }
        }
    }
}
