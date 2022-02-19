using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Data;
using System.Diagnostics;
using System.Drawing;
using System.IO;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using System.Windows.Forms;
using CmlLib;
using CmlLib.Core.Auth;
using CmlLib.Core;
using CmlLib.Core.Downloader;
using CmlLib.Core.Installer;
using CmlLib.Core.Files;
using System.Threading;
using HtmlAgilityPack;
using System.Net;
using MCServerStatus;
using MCServerStatus.Models;
using System.Net.NetworkInformation;

namespace Projects_Launcher.MainWindows
{
    public partial class ProjectsMain : Form
    {
        public static string versiyon;

        public static string sessions;
        MSession session;

        public static string rammiktar;
        public static string height;
        public static string width;
        public static string versiyons;
        public static string rambox;
        public static string widthbox;
        public static string heightbox;

        Ping p = new Ping();

        int pingsayac;

        public static string TextureDizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/resourcepacks";
        string launcherdizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft";

        Uri fabric = new Uri("https://www.dropbox.com/s/agaj6ootu3cmvok/fabric-installer-0.10.2.jar?dl=1");

        Uri modlar = new Uri("https://www.dropbox.com/sh/k7bwyfdywhgpr0m/AACZaJlPzx7sQ3QVTtPNecJMa?dl=1");
        string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
        public ProjectsMain()
        {
            InitializeComponent();
        }

        private void MainWindowss_Load(object sender, EventArgs e)
        {
            ayarlarpanel.Hide();

            path();
            ramlabel.Visible = false;
            widthlabel.Visible = false;
            heightlabel.Visible = false;


            nickname1.Text = ProjectsLauncherLogin.nickname;
            nickname.Text = ProjectsLauncherLogin.nickname;

            if (Properties.Settings.Default.RamMBS != string.Empty)
            {
                ramtextbox.Text = Properties.Settings.Default.RamMBS;
            }
            ramtextbox.MaxLength = 4;

            //ramoku
            if (Properties.Settings.Default.RamMBS != string.Empty)
            {
                ramlabel.Text = Properties.Settings.Default.RamMBS;
            }

            //Resolution
            if (Properties.Settings.Default.ResolutionHeight != string.Empty)
            {
                heightlabel.Text = Properties.Settings.Default.ResolutionHeight;
                heighttextbox.Text = Properties.Settings.Default.ResolutionHeight;
            }
            if (Properties.Settings.Default.ResolutionWidth != string.Empty)
            {
                widthlabel.Text = Properties.Settings.Default.ResolutionWidth;
                widthtextbox.Text = Properties.Settings.Default.ResolutionWidth;
            }

            //versiyon
            if (Properties.Settings.Default.SelectedVersion != string.Empty)
            {
                surumsec.Text = Properties.Settings.Default.SelectedVersion;
                surumtext.Text = Properties.Settings.Default.SelectedVersion;
            }

            //pingsayac
            timer3.Start();

            pingsayac = 0;

        }
        ///////////////
        private void path()
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
            rammiktar = ramlabel.Text;
            height = heightlabel.Text;
            width = widthlabel.Text;
            versiyons = surumtext.Text;

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
                MaximumRamMb = int.Parse(rammiktar),
                Session = MSession.GetOfflineSession(sessions),
                ServerIp = "mc.projects.gg",
                ScreenWidth = int.Parse(height),
                ScreenHeight = int.Parse(width),
            };
            var clientStartProcess = await launcher.CreateProcessAsync(Properties.Settings.Default.SelectedVersion, ayarlar);

            clientStartProcess.Start();

        }
        private async Task ServerStatus()
        {
            IMinecraftPinger pinger = new MinecraftPinger("193.164.7.43", 25565);
            var status = await pinger.RequestAsync();
            String server = status.Players.Online + "";
            serverstatus.Text = server;
        }
        private void timer2_Tick(object sender, EventArgs e)
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
        private void Wc_DownloadFileCompleted(object sender, AsyncCompletedEventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/fabric-installer-0.10.2.jar";

            string myPath = @appDataDizini;
            System.Diagnostics.Process prc = new System.Diagnostics.Process();
            prc.StartInfo.FileName = myPath;
            System.Threading.Thread.Sleep(1000);
            prc.Start();
        }
        private void Wc_DownloadFileCompleted2(object sender, AsyncCompletedEventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/OptiFine_1.18.1_HD_U_H.jar";

            string myPath = @appDataDizini;
            System.Diagnostics.Process prc = new System.Diagnostics.Process();
            prc.StartInfo.FileName = myPath;
            System.Threading.Thread.Sleep(1000);
            prc.Start();
        }
        private void timer2_Tick_1(object sender, EventArgs e)
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

        private void oynabutton_Click(object sender, EventArgs e)
        {
            string surum_appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/versions/" + Properties.Settings.Default.SelectedVersion;
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);

            Uri fabric = new Uri("https://www.dropbox.com/s/agaj6ootu3cmvok/fabric-installer-0.10.2.jar?dl=1");
            Uri optifine = new Uri("https://www.dropbox.com/s/lyorwo4m91luhzd/OptiFine_1.18.1_HD_U_H4.jar?dl=1");

            //fabric seçiliyse
            if (Properties.Settings.Default.SelectedVersion == "fabric-loader-0.13.2-1.18.1")
            {
                if (Directory.Exists(@surum_appDataDizini))
                {

                    session = MSession.GetOfflineSession(ProjectsLauncherLogin.nickname);

                    Thread thread = new Thread(() => Launch());
                    thread.IsBackground = true;
                    thread.Start();

                    timer2.Enabled = true;

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

            //optifine seçiliyse
            if (Properties.Settings.Default.SelectedVersion == "OptiFine_1.18.1_HD_U_H4")
            {
                if (Directory.Exists(@surum_appDataDizini))
                {

                    session = MSession.GetOfflineSession(ProjectsLauncherLogin.nickname);

                    Thread thread = new Thread(() => Launch());
                    thread.IsBackground = true;
                    thread.Start();

                    timer2.Enabled = true;

                }
                else
                {

                    DialogResult secenek = MessageBox.Show("Bazı Dosyalar Bulunamadı! İndirmek ister misiniz?", "Projects Launcher", MessageBoxButtons.YesNo);

                    if (secenek == DialogResult.Yes)
                    {
                        WebClient wc = new WebClient();
                        wc.DownloadFileCompleted += Wc_DownloadFileCompleted2;
                        wc.DownloadFileAsync(optifine, appDataDizini + "/.minecraft/OptiFine_1.18.1_HD_U_H4.jar");
                    }
                    else if (secenek == DialogResult.No)
                    {
                        //Hayır seçeneğine tıklandığında çalıştırılacak kodlar
                    }
                }
            }
        }

        private void ayarlarbutton_Click(object sender, EventArgs e)
        {
            ayarlarpanel.Show();
            anamenü.ForeColor = System.Drawing.Color.FromArgb(250, 235, 246);
            ayarlarbutton.ForeColor = System.Drawing.Color.FromArgb(150, 150, 150);
        }
        private void anamenü_Click(object sender, EventArgs e)
        {
            ayarlarpanel.Hide();
            ayarlarbutton.ForeColor = System.Drawing.Color.FromArgb(250, 235, 246);
            anamenü.ForeColor = System.Drawing.Color.FromArgb(150, 150, 150);
        }

        private void guna2ControlBox1_Click(object sender, EventArgs e)
        {
            Application.Exit();
        }

        private async void timer1_Tick(object sender, EventArgs e)
        {
            try
            {
                await ServerStatus();
            }
            catch
            {
                serverstatus.Hide();
                label1.Hide();
            }
        }

        private void timer3_Tick(object sender, EventArgs e)
        {
            //pingsayac++;

            //     string a, b, c;
            //     PingReply pr = p.Send("mc.projects.gg");
            //      a = pr.Status.ToString();
            //     b = pr.Address.ToString();
            //    c = pr.RoundtripTime.ToString();
            //   serverping.Text = string.Format("{2} ms", a, b, c);
        }

        private void ayarlarbutton_MouseEnter(object sender, EventArgs e)
        {
            ayarlarbutton.ForeColor = System.Drawing.Color.FromArgb(255, 255, 255);
        }

        private void ayarlarbutton_MouseLeave(object sender, EventArgs e)
        {
            if (ayarlarpanel.Visible == true)
            {
                ayarlarbutton.ForeColor = System.Drawing.Color.FromArgb(150, 150, 150);
            }
            else
            {
                ayarlarbutton.ForeColor = System.Drawing.Color.FromArgb(235, 235, 235);
            }
        }


        private void anamenü_MouseEnter(object sender, EventArgs e)
        {
            if (ayarlarpanel.Visible == false)
            {
                anamenü.ForeColor = System.Drawing.Color.FromArgb(255, 255, 255);
            }
            else
            {
                anamenü.ForeColor = System.Drawing.Color.FromArgb(150, 150, 150);
            }
        }

        private void anamenü_MouseLeave(object sender, EventArgs e)
        {
            if (ayarlarpanel.Visible == true)
            {
                anamenü.ForeColor = System.Drawing.Color.FromArgb(235, 235, 235);
            }
            else
            {
                anamenü.ForeColor = System.Drawing.Color.FromArgb(150, 150, 150);
            }
        }

        private void label7_Click(object sender, EventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/screenshots";

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

        private void label8_Click(object sender, EventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/logs";

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

        private void label9_Click(object sender, EventArgs e)
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

        private void label10_Click(object sender, EventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/Saves";

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

        private void texturepackyukle_Click(object sender, EventArgs e)
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
        private void widthtextbox_TextChanged(object sender, EventArgs e)
        {
            widthbox = widthtextbox.Text;
            Properties.Settings.Default.ResolutionWidth = widthbox;
            Properties.Settings.Default.Save();
        }

        private void heighttextbox_TextChanged(object sender, EventArgs e)
        {
            heightbox = widthtextbox.Text;
            Properties.Settings.Default.ResolutionHeight = heightbox;
            Properties.Settings.Default.Save();
        }

        private void ramtextbox_TextChanged(object sender, EventArgs e)
        {
            rambox = ramtextbox.Text;
            Properties.Settings.Default.RamMBS = rambox;
            Properties.Settings.Default.Save();
            ramlabel.Text = Properties.Settings.Default.RamMBS;
        }

        private void guna2PictureBox1_Click(object sender, EventArgs e)
        {
            if (raminfopanel.Visible == false)
            {
                raminfopanel.Visible = true;
            }
            else
            {
                raminfopanel.Visible = false;
            }
        }

        private void surumsec_SelectedIndexChanged(object sender, EventArgs e)
        {
            Properties.Settings.Default.SelectedVersion = surumsec.Text;
            Properties.Settings.Default.Save();
            surumtext.Text = Properties.Settings.Default.SelectedVersion;
        }

        private void fabricmodpack_Click(object sender, EventArgs e)
        {
            ProjectsLauncherDownload download = new ProjectsLauncherDownload();
            if(download.Visible == true)
            {
                download.Hide();
            }
            else
            {
                download.Show();
            }
        }
        private void indirbutton_Click(object sender, EventArgs e)
        {
            WebClient wc = new WebClient();
            wc.DownloadProgressChanged += Wc_DownloadProgressChanged3;
            wc.DownloadFileCompleted += Wc_DownloadFileCompleted3;
            wc.DownloadFileAsync(fabric, appDataDizini + "/.minecraft/fabric-installer-0.10.2.jar");
        }

        private void indirbutton2_Click(object sender, EventArgs e)
        {
            WebClient wc = new WebClient();
            wc.DownloadProgressChanged += Wc_DownloadProgressChanged4;
            wc.DownloadFileCompleted += Wc_DownloadFileCompleted4;
            wc.DownloadFileAsync(modlar, Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/mods.zip");
        }

        private void Wc_DownloadFileCompleted3(object sender, AsyncCompletedEventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/fabric-installer-0.10.2.jar";
            string myPath = @appDataDizini;
            System.Diagnostics.Process prc = new System.Diagnostics.Process();
            prc.StartInfo.FileName = myPath;
            System.Threading.Thread.Sleep(1000);
            prc.Start();
        }
        private void Wc_DownloadFileCompleted4(object sender, AsyncCompletedEventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/mods.zip";


            string myPath = @appDataDizini;
            System.Diagnostics.Process prc = new System.Diagnostics.Process();
            prc.StartInfo.FileName = myPath;
            System.Threading.Thread.Sleep(1000);
            prc.Start();
        }

        private void Wc_DownloadProgressChanged3(object sender, DownloadProgressChangedEventArgs e)
        {
            yuzde1.Value = e.ProgressPercentage;
            yuzde1.Text = "%0" + e.BytesReceived;

        }



        private void Wc_DownloadProgressChanged4(object sender, DownloadProgressChangedEventArgs e)
        {
            yuzde2.Value = e.ProgressPercentage;
            yuzde2.Text = "%0" + e.BytesReceived;
        }

        private void ramtextbox_TextChanged_1(object sender, EventArgs e)
        {
            rambox = ramtextbox.Text;
            Properties.Settings.Default.RamMBS = rambox;
            Properties.Settings.Default.Save();
            ramlabel.Text = Properties.Settings.Default.RamMBS;
        }
    }
}
