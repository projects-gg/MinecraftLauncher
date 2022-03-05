using CmlLib.Core;
using CmlLib.Core.Auth;
using DiscordRPC;
using MCServerStatus;
using Microsoft.Win32;
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Management;
using System.Net;
using System.Net.NetworkInformation;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace Projects_Launcher.Projects_Launcher
{
    public partial class Anamenu : Form
    {
        public Anamenu()
        {
            InitializeComponent();
        }

        public static string versiyon;

        public static string sessions;
        public static MSession session;

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


        Ping p = new Ping();

        int pingsayac;

        public static string TextureDizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/resourcepacks";
        string launcherdizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects";

        Uri fabric = new Uri("https://www.dropbox.com/s/agaj6ootu3cmvok/fabric-installer-0.10.2.jar?dl=1");

        string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);

        Random rnd = new Random();
        int x, y, z;

        private int uiThreadId = Thread.CurrentThread.ManagedThreadId;
        public DiscordRpcClient Client { get; private set; }
        public void Setup()
        {
            Client = new DiscordRpcClient("949311557542756362");  //Creates the client
            Client.Initialize();                            //Connects the client

            Client.SetPresence(new RichPresence()
            {
                Details = "Anamenüde - Projects Survival",
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
            //Donanım Bilgileri
            try
            {
                //Ekran Kartı
                ManagementObjectSearcher ekran = new ManagementObjectSearcher("Select * From Win32_VideoController");

                foreach (ManagementObject Mobject in ekran.Get())
                {
                    EkranKartıInfo.Text = Mobject["name"].ToString();
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

            pingsayac = 0; //Ping Sayaç
            timer2.Start(); //Ping Sayaç

            nickname1.Text = Properties.Settings.Default.NickNames; //Nickname Bilgisini Göster

            //Oyun Kapanınca Aç / Tick
            try
            {
                if (Properties.Settings.Default.OyunTickS != string.Empty)
                {
                    ticksave.Text = Properties.Settings.Default.OyunTickS;
                }
                if (ticksave.Text == "acik")
                {
                    kapattick.Checked = true;
                }
                if (ticksave.Text == "kapali")
                {
                    kapattick.Checked = false;
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
                    surumt.Text = Properties.Settings.Default.SelectedVersion;
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
                    surumsec.Text = Properties.Settings.Default.SelectedVersion;
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
                    maxramtext.Text = Properties.Settings.Default.RamMax;
                }


                if (Properties.Settings.Default.RamMax != string.Empty)
                {
                    maxramlabel.Text = Properties.Settings.Default.RamMax;
                    try
                    {
                        maxrammb.Text = String.Format("{0:0.##}", Convert.ToDouble(maxramtext.Text) / 1024) + "GB";
                    }
                    catch
                    {
                        maxrammb.Text = "Geçersiz Değer!";
                    }
                }
                else if (maxrammb.Text != "")
                {
                    maxrammb.Text = "";
                }
               

                


                //min
                if (Properties.Settings.Default.RamMin != string.Empty)
                {
                    minramtext.Text = Properties.Settings.Default.RamMin;
                }
                if (Properties.Settings.Default.RamMin != string.Empty)
                {
                    try
                    {
                        minrammb.Text = String.Format("{0:0.##}", Convert.ToDouble(minramtext.Text) / 1024) + "GB";
                    }
                    catch
                    {
                        minrammb.Text = "Geçersiz Değer!";
                    }
                }
                else if (maxrammb.Text != "")
                {
                    minrammb.Text = "";
                }
                minramtext.MaxLength = 4;
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
                var request = WebRequest.Create("https://minotar.net/body" + "/" + nickname1.Text);

                using (var response = request.GetResponse())
                using (var stream = response.GetResponseStream())
                {
                    skin.Image = Bitmap.FromStream(stream);
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
        }
        public async void Launch() //Minecraft Başlatma Ayarları
        {
            var path = new MinecraftPath(launcherdizin);
            var launcher = new CMLauncher(path);
            sessions = ProjectsLauncherLogin.nickname;

            var ayarlar = new MLaunchOption
            {
                MaximumRamMb = int.Parse(Properties.Settings.Default.RamMax), //Maksimum RAM bilgisini al
                MinimumRamMb = int.Parse(Properties.Settings.Default.RamMin), //Minimum RAM bilgisini al
                Session = MSession.GetOfflineSession(sessions), //Nickname bilgisini al
                ServerIp = "mc.projects.gg", //Bağlanılacak sunucu IP adresi
                ScreenWidth = int.Parse(Properties.Settings.Default.ResolutionWidth), //Ekran boyutu bilgisini al
                ScreenHeight = int.Parse(Properties.Settings.Default.ResolutionHeight), //Ekran boyutu bilgisini al
            };
            var clientStartProcess = await launcher.CreateProcessAsync(Properties.Settings.Default.SelectedVersion, ayarlar); //Seçilen versiyon bilgi ve ayarlar ile-

            clientStartProcess.Start(); // Oyunu başlat

            timer1.Enabled = true; //Timer1 i çalıştır

        }


        private void oynabutton_Click(object sender, EventArgs e)
        {
            string surum_appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/versions/fabric-loader-0.13.3-1.18.2"; //Fabric Dizini
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData); //Appdata dizini

            Uri fabric = new Uri("https://www.dropbox.com/s/agaj6ootu3cmvok/fabric-installer-0.10.2.jar?dl=1"); //Fabric İnstaller indirme adresi

            if (Directory.Exists(@surum_appDataDizini)) //Fabric varmı yokmu kontrol et
            {
                try //Eğer fabric varsa
                {
                    session = MSession.GetOfflineSession(ProjectsLauncherLogin.nickname); //Nickname bilgisini al

                    Thread thread = new Thread(() => Launch());
                    thread.IsBackground = true;
                    thread.Start(); //Oyunu başlat

                    surumt.Text = "Başlatılıyor...";
                    this.Enabled = false;
                    timer1.Start(); //Timer1 i başlat
                    
                }
                catch //Eğer fabric yoksa
                {
                    timer1.Stop(); //Timer1 i durdur
                    DialogResult secenek = MessageBox.Show("Oyunu başlatırken bir sorun meydana geldi.", "Bilgi", MessageBoxButtons.OK);

                    if (secenek == DialogResult.OK)
                    {
                        //Burada yazanı yap
                    }
                    this.Enabled = true; //Launcherın bileşenlerini aktifleştir

                    if (Properties.Settings.Default.SelectedVersion != string.Empty)
                    {
                        surumt.Text = Properties.Settings.Default.SelectedVersion; //surumt textine sürüm bilgisini yazdır
                    }

                    surumt.Text = Properties.Settings.Default.SelectedVersion; //surumt textine sürüm bilgisini yazdır
                    
                    this.Enabled = true;

                }


            }
            else
            {

                DialogResult secenek = MessageBox.Show("Bazı Dosyalar Bulunamadı! İndirmek ister misiniz?", "Projects Launcher", MessageBoxButtons.YesNo); //Fabric dosyasının olmadığını bildir

                if (secenek == DialogResult.Yes) //MessageBox da evete tıklanırsa
                {
                    WebClient wc = new WebClient(); //Webclient çağır
                    wc.DownloadFileCompleted += Wc_DownloadFileCompleted; //İndirme işlemi bitince çalıştırılacak kodları çağır
                    wc.DownloadFileAsync(fabric, appDataDizini + "/.projects/fabric-installer-0.10.2.jar"); //fabric dizinine fabric'i indir
                }
                else if (secenek == DialogResult.No) //MessageBox da hayıra tıklanırsa
                {
                    //Hayır seçeneğine tıklandığında çalıştırılacak kodlar
                }
            }
        }
        private void Wc_DownloadFileCompleted(object sender, AsyncCompletedEventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/fabric-installer-0.10.2.jar";

            string myPath = @appDataDizini;
            System.Diagnostics.Process prc = new System.Diagnostics.Process();
            prc.StartInfo.FileName = myPath;
            System.Threading.Thread.Sleep(1031);
            prc.Start();
        }

        private void timer1_Tick(object sender, EventArgs e)
        {
            if (kapattick.Checked == true)
            {
                foreach (var process in Process.GetProcessesByName("javaw"))
                {
                    Thread.Sleep(1031);
                    surumt.Text = "Başlatılıyor...";
                    oynabutton.Enabled = false;
                    this.Visible = false;
                    Thread.Sleep(2000);
                    timer3.Start();
                }
            }
            else
            {
                foreach (var process in Process.GetProcessesByName("javaw"))
                {
                    Thread.Sleep(1031);
                    surumt.Text = "Başlatılıyor...";
                    oynabutton.Enabled = false;
                    this.Visible = false;
                    timer3.Stop();
                    Environment.Exit(0);

                }
            }
        }

        private void ayarlarbutton_Click(object sender, EventArgs e)
        {
            if (panel213.Visible == false)
            {
                geriformpanel.Visible = true;
                panel213.Visible = true;
            }
            else
            {
                panel213.Visible = false;
            }


        }

        private async Task ServerStatus()
        {
            try
            {
                IMinecraftPinger pinger = new MinecraftPinger("193.164.7.43", 25565);
                var status = await pinger.RequestAsync();
                String sses = status.ToString();
                String server = status.Players.Online + "";
                serverplayer.Text = (server + " Kişi aktif!");
            }
            catch
            {
                serverplayer.Text = ("Serverdan Bilgi Alınamadı");
            }

        }

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

            }


        }

        private void guna2ControlBox1_Click(object sender, EventArgs e)
        {
            Environment.Exit(0);
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
            formpanell = panel213.Visible;
        }

        private void geriformpanel_Click(object sender, EventArgs e)
        {
            panel213.Visible = false;
            geriformpanel.Visible = false;
        }

        private void widthtextbox_TextChanged(object sender, EventArgs e)
        {
            widthbox = heighttextbox.Text;
            Properties.Settings.Default.ResolutionWidth = widthbox;
            Properties.Settings.Default.Save();
            Projects_Launcher.Anamenu.widthlabell = Properties.Settings.Default.ResolutionWidth;
        }

        private void heighttextbox_TextChanged(object sender, EventArgs e)
        {
            heightbox = heighttextbox.Text;
            Properties.Settings.Default.ResolutionHeight = heightbox;
            Properties.Settings.Default.Save();
            Projects_Launcher.Anamenu.heightlabell = Properties.Settings.Default.ResolutionHeight;
        }

        private void maxramtext_TextChanged(object sender, EventArgs e)
        {
            maxrambox = maxramtext.Text;
            Properties.Settings.Default.RamMax = maxrambox;
            Properties.Settings.Default.Save();
            maxramlabel.Text = Properties.Settings.Default.RamMax;

            //GB Convert
            if (Properties.Settings.Default.RamMax != string.Empty)
            {
                maxramlabel.Text = Properties.Settings.Default.RamMax;
                try
                {
                    maxrammb.Text = String.Format("{0:0.##}", Convert.ToDouble(maxramtext.Text) / 1024) + "GB";
                }
                catch
                {
                    maxrammb.Text = "Geçersiz Değer!";
                }
            }
            else if (maxrammb.Text != "")
            {
                maxrammb.Text = "";
            }
        }



        private void surumsec_SelectedIndexChanged(object sender, EventArgs e)
        {
            Properties.Settings.Default.SelectedVersion = surumsec.Text;
            Properties.Settings.Default.Save();
            surumt.Text = surumsec.Text;
        }

        private void website_Click(object sender, EventArgs e)
        {
            System.Diagnostics.Process.Start("https://mc.projects.gg/");
        }

        private void discord_Click(object sender, EventArgs e)
        {
            System.Diagnostics.Process.Start("https://discord.com/invite/9hxHCTQ");
        }

        private void instagram_Click(object sender, EventArgs e)
        {
            System.Diagnostics.Process.Start("https://www.instagram.com/projects.com.tr/");
        }

        private void geriformpanel_MouseEnter(object sender, EventArgs e)
        {
            x = rnd.Next(255);
            y = rnd.Next(255);
            z = rnd.Next(255);
            geriformpanel.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
        }

        private void geriformpanel_MouseLeave(object sender, EventArgs e)
        {
            geriformpanel.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
        }

        private void changelogs_MouseEnter(object sender, EventArgs e)
        {
            x = rnd.Next(255);
            y = rnd.Next(255);
            z = rnd.Next(255);
            geriformpanel.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
        }

        private void changelogs_MouseLeave(object sender, EventArgs e)
        {
            oynabutton.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
        }
        private void minramtext_TextChanged(object sender, EventArgs e)
        {
            minrambox = minramtext.Text;
            Properties.Settings.Default.RamMin = minrambox;
            Properties.Settings.Default.Save();
            minramlabel.Text = Properties.Settings.Default.RamMin;

            //GB Convert
            if (Properties.Settings.Default.RamMin != string.Empty)
            {
                try
                {
                    minrammb.Text = String.Format("{0:0.##}", Convert.ToDouble(minramtext.Text) / 1024) + "GB";
                }
                catch
                {
                    minrammb.Text = "Geçersiz Değer!";
                }
            }
            else if (maxrammb.Text != "")
            {
                minrammb.Text = "";
            }
        }

        private void minramlabel_Click(object sender, EventArgs e)
        {
            minramlabell = minramlabel.Text;
        }

        private void mods_MouseEnter(object sender, EventArgs e)
        {
            x = rnd.Next(255);
            y = rnd.Next(255);
            z = rnd.Next(255);
            mods.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
        }

        private void mods_MouseLeave(object sender, EventArgs e)
        {
            mods.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
        }

        private void texturepackfolder_MouseEnter(object sender, EventArgs e)
        {
            x = rnd.Next(255);
            y = rnd.Next(255);
            z = rnd.Next(255);
            texturepackfolder.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
        }

        private void texturepackfolder_MouseLeave(object sender, EventArgs e)
        {
            texturepackfolder.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
        }

        private void texturepackaktar_MouseEnter(object sender, EventArgs e)
        {
            x = rnd.Next(255);
            y = rnd.Next(255);
            z = rnd.Next(255);
            texturepackaktar.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
        }

        private void texturepackaktar_MouseLeave(object sender, EventArgs e)
        {
            texturepackaktar.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
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
                        MessageBox.Show(DosyaAdi + " isimli TexturePack zaten mevcut.", "", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                    }
                    else
                    {
                        File.Copy(DosyaYolu, TextureDizin + "\\" + DosyaAdi);
                        MessageBox.Show("TexturePack başarıyla yüklendi.");
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
                        MessageBox.Show(DosyaAdi + " isimli TexturePack zaten mevcut.", "", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                    }
                    else
                    {
                        File.Copy(DosyaYolu, TextureDizin + "\\" + DosyaAdi);
                        MessageBox.Show("TexturePack başarıyla yüklendi.");
                    }
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
            x = rnd.Next(255);
            y = rnd.Next(255);
            z = rnd.Next(255);
            gamefolder.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
        }

        private void gamefolder_MouseLeave(object sender, EventArgs e)
        {
            gamefolder.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
        }

        private void changelogst_Click(object sender, EventArgs e)
        {

        }

        private void timer3_Tick(object sender, EventArgs e)
        {
            if (!Process.GetProcessesByName("javaw").Any())
            {
                if (Properties.Settings.Default.SelectedVersion != string.Empty)
                {
                    surumt.Text = Properties.Settings.Default.SelectedVersion;
                }
                oynabutton.Enabled = true;
                this.Visible = true;
                this.Enabled = true;
                timer1.Stop();
            }
            else
            {
                timer1.Start();
            }
        }

        private void kapattick_CheckedChanged(object sender, EventArgs e)
        {

            if (kapattick.Checked == true)
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

        private void maxramtext_Leave(object sender, EventArgs e)
        {
            ManagementObjectSearcher Search = new ManagementObjectSearcher("Select * From Win32_ComputerSystem");
            foreach (ManagementObject Mobject in Search.Get())
            {

                double Ram_Bytes = (Convert.ToDouble(Mobject["TotalPhysicalMemory"]));
                double ramgb = Ram_Bytes / 1073741824;
                double islem = Math.Ceiling(ramgb);
                rambilgi = String.Format("{0:0.##}", Convert.ToDouble(islem) * 1024 - 1024);
            }

            if (Convert.ToInt32(maxramtext.Text) < 1024 || Convert.ToInt32(maxramtext.Text) > Convert.ToInt32(rambilgi))
            {
                MessageBox.Show("Miktar 1024-" + rambilgi + " " + "arasında girilmeli.");
                maxramtext.Text = rambilgi;
            }
        }
        private void minramtext_Leave(object sender, EventArgs e)
        {
            ManagementObjectSearcher Search = new ManagementObjectSearcher("Select * From Win32_ComputerSystem");
            foreach (ManagementObject Mobject in Search.Get())
            {

                double Ram_Bytes = (Convert.ToDouble(Mobject["TotalPhysicalMemory"]));
                double ramgb = Ram_Bytes / 1073741824;
                double islem = Math.Ceiling(ramgb);
                rambilgi = String.Format("{0:0.##}", Convert.ToDouble(islem) * 512);
            }

            if (Convert.ToInt32(minramtext.Text) < 1024 || Convert.ToInt32(minramtext.Text) > Convert.ToInt32(rambilgi))
            {
                MessageBox.Show("Miktar 1024-" + rambilgi + " " + "arasında girilmeli.");
                minramtext.Text = rambilgi;
            }
        }

        private void oynabutton_MouseEnter(object sender, EventArgs e)
        {
            x = rnd.Next(255);
            y = rnd.Next(255);
            z = rnd.Next(255);
            oynabutton.ForeColor = System.Drawing.Color.FromArgb(x, y, z);
        }

        private void oynabutton_MouseLeave(object sender, EventArgs e)
        {
            oynabutton.ForeColor = System.Drawing.Color.FromArgb(245, 245, 245);
        }
    }
}
