using CmlLib.Core;
using CmlLib.Core.Auth;
using MCServerStatus;
using System;
using System.ComponentModel;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Net;
using System.Net.NetworkInformation;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;
using CmlLib.Core.Downloader;

namespace Projects_Launcher.MainWindows
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


        Ping p = new Ping();

        int pingsayac;

        public static string TextureDizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/resourcepacks";
        string launcherdizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft";

        Uri fabric = new Uri("https://www.dropbox.com/s/agaj6ootu3cmvok/fabric-installer-0.10.2.jar?dl=1");

        Uri modlar = new Uri("https://www.dropbox.com/sh/k7bwyfdywhgpr0m/AACZaJlPzx7sQ3QVTtPNecJMa?dl=1");

        string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);

        Random rnd = new Random();
        int x, y, z;

        private int uiThreadId = Thread.CurrentThread.ManagedThreadId;
        private void Anamenu_Load(object sender, EventArgs e)
        {
            nickname1.Text = ProjectsLauncherLogin.nickname;

            pingsayac = 0;
            timer2.Start();

            //versiyon
            if (Properties.Settings.Default.SelectedVersion != string.Empty)
            {
                surumt.Text = Properties.Settings.Default.SelectedVersion;
            }

            //ram
            if (Properties.Settings.Default.RamMax != string.Empty)
            {
                maxramtext.Text = Properties.Settings.Default.RamMax;
            }
            maxramtext.MaxLength = 4;
            if (Properties.Settings.Default.RamMin != string.Empty)
            {
                minramtext.Text = Properties.Settings.Default.RamMin;
            }
            minramtext.MaxLength = 4;

            //Resolution
            if (Properties.Settings.Default.ResolutionHeight != string.Empty)
            {
                heighttextbox.Text = Properties.Settings.Default.ResolutionHeight;
            }
            if (Properties.Settings.Default.ResolutionWidth != string.Empty)
            {
                widthtextbox.Text = Properties.Settings.Default.ResolutionWidth;
            }

            //versiyon
            if (Properties.Settings.Default.SelectedVersion != string.Empty)
            {
                surumsec.Text = Properties.Settings.Default.SelectedVersion;
            }

            var request = WebRequest.Create("https://minotar.net/body/" + "/" + nickname1.Text);

            using (var response = request.GetResponse())
            using (var stream = response.GetResponseStream())
            {
                skin.Image = Bitmap.FromStream(stream);
            }
        }
        public void path()
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
        public async void Launch()
        {

            //var path = new MinecraftPath();



            var path = new MinecraftPath(launcherdizin);
            var launcher = new CMLauncher(path);
            //versiyon = versiyonselect.SelectedItem.ToString();
            sessions = ProjectsLauncherLogin.nickname;

            launcher.FileChanged += (e) =>
            {
                //log.Text = (string.Format("[{0}] {1} - {2}/{3}", e.FileKind.ToString(), e.FileName, e.ProgressedFileCount, e.TotalFileCount));
            };

            var ayarlar = new MLaunchOption
            {
                MaximumRamMb = int.Parse(Properties.Settings.Default.RamMax),
                MinimumRamMb = int.Parse(Properties.Settings.Default.RamMin),
                Session = MSession.GetOfflineSession(sessions),
                ServerIp = "mc.projects.gg",
                ScreenWidth = int.Parse(Properties.Settings.Default.ResolutionWidth),
                ScreenHeight = int.Parse(Properties.Settings.Default.ResolutionHeight),
            };
            var clientStartProcess = await launcher.CreateProcessAsync(Properties.Settings.Default.SelectedVersion, ayarlar);

            clientStartProcess.Start();

        }
        private void oynabutton_Click(object sender, EventArgs e)
        {
            string surum_appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/versions/" + Properties.Settings.Default.SelectedVersion;
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);

            Uri fabric = new Uri("https://www.dropbox.com/s/agaj6ootu3cmvok/fabric-installer-0.10.2.jar?dl=1");
            if (Directory.Exists(@surum_appDataDizini))
            {

                session = MSession.GetOfflineSession(ProjectsLauncherLogin.nickname);

                Thread thread = new Thread(() => Launch());
                thread.IsBackground = true;
                thread.Start();

                timer1.Enabled = true;

            }
            else
            {

                DialogResult secenek = MessageBox.Show("Bazı Dosyalar Bulunamadı! İndirmek ister misiniz?", "Projects Launcher", MessageBoxButtons.YesNo);

                if (secenek == DialogResult.Yes)
                {
                    WebClient wc = new WebClient();
                    wc.DownloadFileCompleted += Wc_DownloadFileCompleted;
                    wc.DownloadFileAsync(fabric, appDataDizini + "/.minecraft/fabric-installer-0.10.2.jar");
                }
                else if (secenek == DialogResult.No)
                {
                    //Hayır seçeneğine tıklandığında çalıştırılacak kodlar
                }
            }
        }
        private void Wc_DownloadFileCompleted(object sender, AsyncCompletedEventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/fabric-installer-0.10.2.jar";

            string myPath = @appDataDizini;
            System.Diagnostics.Process prc = new System.Diagnostics.Process();
            prc.StartInfo.FileName = myPath;
            System.Threading.Thread.Sleep(1000);
            prc.Start();
        }

        private void timer1_Tick(object sender, EventArgs e)
        {

            foreach (var process in Process.GetProcessesByName("javaw"))
            {
                oynabutton.Text = "Başlatılıyor...";
                oynabutton.Enabled = false;
                this.Visible = false;
            }


            if (!Process.GetProcessesByName("javaw").Any())
            {
                oynabutton.Text = "Oyna";
                oynabutton.Enabled = true;
                this.Visible = true;
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
            IMinecraftPinger pinger = new MinecraftPinger("193.164.7.43", 25565);
            var status = await pinger.RequestAsync();
            String server = status.Players.Online + "";
            serverplayer.Text = (server + " Kişi aktif!");
        }

        private async void timer2_Tick(object sender, EventArgs e)
        {
            
            try
            {
                //ping
                pingsayac++;

                string a, b, c;
                PingReply pr = p.Send("mc.projects.gg");
                a = pr.Status.ToString();
                b = pr.Address.ToString();
                c = pr.RoundtripTime.ToString();
                pingsayacc.Text = string.Format("{2} ms", a, b, c);

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
            widthbox = widthtextbox.Text;
            Properties.Settings.Default.ResolutionWidth = widthbox;
            Properties.Settings.Default.Save();
            MainWindows.Anamenu.widthlabell = Properties.Settings.Default.ResolutionWidth;
        }

        private void heighttextbox_TextChanged(object sender, EventArgs e)
        {
            heightbox = widthtextbox.Text;
            Properties.Settings.Default.ResolutionHeight = heightbox;
            Properties.Settings.Default.Save();
            MainWindows.Anamenu.heightlabell = Properties.Settings.Default.ResolutionHeight;
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
            geriformpanel.ForeColor = Color.FromArgb(x, y, z);
        }

        private void geriformpanel_MouseLeave(object sender, EventArgs e)
        {
            geriformpanel.ForeColor = Color.FromArgb(245, 245, 245);
        }

        private void changelogs_MouseEnter(object sender, EventArgs e)
        {
            x = rnd.Next(255);
            y = rnd.Next(255);
            z = rnd.Next(255);
            geriformpanel.ForeColor = Color.FromArgb(x, y, z);
        }

        private void changelogs_MouseLeave(object sender, EventArgs e)
        {
            oynabutton.ForeColor = Color.FromArgb(245, 245, 245);
        }

        private void changelogs_Click(object sender, EventArgs e)
        {
            if (changelogst.Visible == false)
            {
                changelogst.Visible = true;
            }
            else
            {
                changelogst.Visible = false;
            }
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
            mods.ForeColor = Color.FromArgb(x, y, z);
        }

        private void mods_MouseLeave(object sender, EventArgs e)
        {
            mods.ForeColor = Color.FromArgb(245, 245, 245);
        }

        private void texturepackfolder_MouseEnter(object sender, EventArgs e)
        {
            x = rnd.Next(255);
            y = rnd.Next(255);
            z = rnd.Next(255);
            texturepackfolder.ForeColor = Color.FromArgb(x, y, z);
        }

        private void texturepackfolder_MouseLeave(object sender, EventArgs e)
        {
            texturepackfolder.ForeColor = Color.FromArgb(245, 245, 245);
        }

        private void texturepackaktar_MouseEnter(object sender, EventArgs e)
        {
            x = rnd.Next(255);
            y = rnd.Next(255);
            z = rnd.Next(255);
            texturepackaktar.ForeColor = Color.FromArgb(x, y, z);
        }

        private void texturepackaktar_MouseLeave(object sender, EventArgs e)
        {
            texturepackaktar.ForeColor = Color.FromArgb(245, 245, 245);
        }

        private void mods_Click(object sender, EventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/mods";

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
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft";

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
            gamefolder.ForeColor = Color.FromArgb(x, y, z);
        }

        private void gamefolder_MouseLeave(object sender, EventArgs e)
        {
            gamefolder.ForeColor = Color.FromArgb(245, 245, 245);
        }

        private void oynabutton_MouseEnter(object sender, EventArgs e)
        {
            x = rnd.Next(255);
            y = rnd.Next(255);
            z = rnd.Next(255);
            oynabutton.ForeColor = Color.FromArgb(x, y, z);
        }

        private void oynabutton_MouseLeave(object sender, EventArgs e)
        {
            oynabutton.ForeColor = Color.FromArgb(245, 245, 245);
        }
    }
}
