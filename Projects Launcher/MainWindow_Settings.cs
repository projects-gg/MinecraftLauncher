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
namespace Projects_Launcher
{
    public partial class ProjectsLauncherOptions : Form
    {
        public ProjectsLauncherOptions()
        {
            InitializeComponent();
        }
        public static string versiyon;
        public static string ram;
        public static string sessions;
        public static string rambox;
        public static string widthbox;
        public static string heightbox;
        public static string rammiktar;
        public static string height;
        public static string width;
        public static string versiyons;


        MSession session;

        public static string TextureDizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/resourcepacks";
        string launcherdizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects";

        private void ProjectsLauncherOptions_Load(object sender, EventArgs e)
        {
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
        }

        private void label7_Click(object sender, EventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/screenshots";

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
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/logs";

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

        private void label10_Click(object sender, EventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/Saves";

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

        private void surumsec_SelectedIndexChanged(object sender, EventArgs e)
        {

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
            Properties.Settings.Default.Save(); ;
            if (Properties.Settings.Default.RamMBS != string.Empty)
            {
                ramlabel.Text = Properties.Settings.Default.RamMBS;
            }
        }

        private void indirmeler_Click_1(object sender, EventArgs e)
        {
            ProjectsLauncherDownload Download = new ProjectsLauncherDownload();
            Download.Show();
        }

        private void bilgibutton_Click_1(object sender, EventArgs e)
        {
            ProjectsLauncherInfo info = new ProjectsLauncherInfo();
            this.Hide();
            info.Show();
        }

        private void anamenü_Click_1(object sender, EventArgs e)
        {
            ProjectsLauncherMain main = new ProjectsLauncherMain();
            this.Hide();
            main.Show();
        }

        private void path()
        {
            System.Net.ServicePointManager.DefaultConnectionLimit = 256;

            var path = new MinecraftPath(launcherdizin);
            //var path = new MinecraftPath();

            var launcher = new CMLauncher(path);

            // foreach (var item in launcher.GetAllVersions())
            //{
            //   surumsec.Items.Add(item.Name);
            //}
        }

        async public void Launch()
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

            };

            var ayarlar = new MLaunchOption
            {
                MaximumRamMb = int.Parse(rammiktar),
                Session = MSession.GetOfflineSession(sessions),
                ServerIp = "mc.projects.gg",
                ScreenWidth = int.Parse(height),
                ScreenHeight = int.Parse(width),
            };
            var proces = await launcher.CreateProcessAsync(surumtext.Text, ayarlar);

            proces.Start();
        }

        private void ramtextbox_SelectedIndexChanged(object sender, EventArgs e)
        {

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

        private void guna2ControlBox1_Click_1(object sender, EventArgs e)
        {
            Application.Exit();
        }

        private void oynabutton_Click(object sender, EventArgs e)
        {
            string fabric_appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/versions/fabric-loader-0.13.1-1.18.1";
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            Uri fabric = new Uri("https://www.dropbox.com/s/agaj6ootu3cmvok/fabric-installer-0.10.2.jar?dl=1");

            if (Directory.Exists(@fabric_appDataDizini))
            {

                session = MSession.GetOfflineSession(ProjectsLauncherLogin.nickname);
                oynabutton.Text = "Başlatılıyor...";
                oynabutton.Enabled = false;

                Thread thread = new Thread(() => Launch());
                thread.IsBackground = true;
                thread.Start();

                System.Threading.Thread.Sleep(6500);

                for (int i = 0; i <= 100; i++)
                {
                    foreach (var process in Process.GetProcessesByName("javaw"))
                    {
                        Application.Exit();
                    }
                }
            }
            else
            {

                DialogResult secenek = MessageBox.Show("Bazı Dosyalar Bulunamadı! İndirmek ister misiniz?", "Projects Launcher", MessageBoxButtons.YesNo);

                if (secenek == DialogResult.Yes)
                {
                    WebClient wc = new WebClient();
                    wc.DownloadFileCompleted += Wc_DownloadFileCompleted;
                    wc.DownloadFileAsync(fabric, appDataDizini + "/.projects/fabric-installer-0.10.2.jar");
                }
                else if (secenek == DialogResult.No)
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
            System.Threading.Thread.Sleep(1000);
            prc.Start();


        }

        private async Task ServerStatus()
        {
            IMinecraftPinger pinger = new MinecraftPinger("193.164.7.43", 25565);
            var status = await pinger.RequestAsync();
            String server = status.Players.Online + "";
            serverstatus.Text = server;
        }

        private async void timer1_Tick(object sender, EventArgs e)
        {
            await ServerStatus();
        }
    }
}
