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
namespace Projects_Launcher
{
    public partial class ProjectsLauncherInfo : Form
    {
        public ProjectsLauncherInfo()
        {
            InitializeComponent();
            Control.CheckForIllegalCrossThreadCalls = false;
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

        Ping p = new Ping();

        int pingsayac;

        MSession session;
        string launcherdizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft";
        private void ProjectsLauncherInfo_Load(object sender, EventArgs e)
        {
            ramlabel.Visible = false;
            widthlabel.Visible = false;
            heightlabel.Visible = false;


            nickname1.Text = ProjectsLauncherLogin.nickname;

            path();

            //ramoku
            if (Properties.Settings.Default.RamMBS != string.Empty)
            {
                ramlabel.Text = Properties.Settings.Default.RamMBS;
            }

            //Resolution
            if (Properties.Settings.Default.ResolutionHeight != string.Empty)
            {
                heightlabel.Text = Properties.Settings.Default.ResolutionHeight;
            }
            if (Properties.Settings.Default.ResolutionWidth != string.Empty)
            {
                widthlabel.Text = Properties.Settings.Default.ResolutionWidth;
            }

            //versiyon
            if (Properties.Settings.Default.SelectedVersion != string.Empty)
            {
                surumtext.Text = Properties.Settings.Default.SelectedVersion;
            }

            //pingsayac
            timer3.Start();

            pingsayac = 0;

        }

        private void label9_Click(object sender, EventArgs e)
        {
            ProjectsLauncherDownload Download = new ProjectsLauncherDownload();
            Download.Show();
        }

        private void label10_Click(object sender, EventArgs e)
        {
            System.Diagnostics.Process.Start("https://mc.projects.gg/");
        }

        private void label12_Click(object sender, EventArgs e)
        {
            ProjectsLauncherDownload Download = new ProjectsLauncherDownload();
            Download.Show();

        }

        private void label14_Click(object sender, EventArgs e)
        {
            System.Diagnostics.Process.Start("https://www.minecraft.net/tr-tr");
        }

        private void label13_Click(object sender, EventArgs e)
        {
            System.Diagnostics.Process.Start("https://discord.com/invite/9hxHCTQ");
        }

        private void ayarlarbutton_Click(object sender, EventArgs e)
        {
            ProjectsLauncherOptions Options = new ProjectsLauncherOptions();
            this.Hide();
            Options.Show();
        }

        private void anamenü_Click(object sender, EventArgs e)
        {
            ProjectsLauncherMain main = new ProjectsLauncherMain();
            this.Hide();
            main.Show();
        }

        private void indirmeler_Click(object sender, EventArgs e)
        {
            ProjectsLauncherDownload Download = new ProjectsLauncherDownload();
            Download.Show();
        }

        private void oynabutton_Click(object sender, EventArgs e)
        {
            string fabric_appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/versions/" + Properties.Settings.Default.SelectedVersion;
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
        private void timer2_Tick(object sender, EventArgs e)
        {
            foreach (var process in Process.GetProcessesByName("javaw"))
            {
                this.Hide();
            }

            if (!Process.GetProcessesByName("javaw").Any())
            {
                oynabutton.Text = "Oyna";
                oynabutton.Enabled = true;
                this.Show();
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

        private void path()
        {
            System.Net.ServicePointManager.DefaultConnectionLimit = 256;

            var path = new MinecraftPath(launcherdizin);
            //var path = new MinecraftPath();

            var launcher = new CMLauncher(path);

            foreach (var item in launcher.GetAllVersions())
            {
                //versiyonselect.Items.Add(item.Name);
            }
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
            var clientLaunchProcess = await launcher.CreateProcessAsync(surumtext.Text, ayarlar);

            clientLaunchProcess.Start();
        }

        private async Task ServerStatus()
        {
            IMinecraftPinger pinger = new MinecraftPinger("193.164.7.43", 25565);
            var status = await pinger.RequestAsync();
            serverstatus.Text = Convert.ToString(status.Players.Online);
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

        private void guna2ControlBox1_Click(object sender, EventArgs e)
        {
            Application.Exit();
        }

        private void anamenü_MouseEnter(object sender, EventArgs e)
        {
            anamenü.ForeColor = System.Drawing.Color.FromArgb(255, 255, 255);
        }

        private void anamenü_MouseLeave(object sender, EventArgs e)
        {
            anamenü.ForeColor = System.Drawing.Color.FromArgb(250, 235, 246);
        }

        private void indirmeler_MouseLeave(object sender, EventArgs e)
        {
            indirmeler.ForeColor = System.Drawing.Color.FromArgb(250, 235, 246);
        }

        private void indirmeler_MouseEnter(object sender, EventArgs e)
        {
            indirmeler.ForeColor = System.Drawing.Color.FromArgb(255, 255, 255);
        }

        private void ayarlarbutton_MouseEnter(object sender, EventArgs e)
        {
            ayarlarbutton.ForeColor = System.Drawing.Color.FromArgb(255, 255, 255);
        }

        private void ayarlarbutton_MouseLeave(object sender, EventArgs e)
        {
            ayarlarbutton.ForeColor = System.Drawing.Color.FromArgb(250, 235, 246);
        }

        private void timer3_Tick(object sender, EventArgs e)
        {
            pingsayac++;

            string a, b, c;
            PingReply pr = p.Send("mc.projects.gg");
            a = pr.Status.ToString();
            b = pr.Address.ToString();
            c = pr.RoundtripTime.ToString();
            serverping.Text = string.Format("{2} ms", a, b, c);

        }
    }
}
