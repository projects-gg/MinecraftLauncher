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

        Random rnd = new Random();

        public static string surumtextt;
        public static string widthlabell;
        public static string heightlabell;
        public static string ramlabell;

        public ProjectsMain()
        {
            InitializeComponent();
        }

        private void MainWindowss_Load(object sender, EventArgs e)
        {
            int x = rnd.Next(255);
            int y = rnd.Next(255);
            int z = rnd.Next(255);

            anamenü.ForeColor = System.Drawing.Color.FromArgb(x, y, z);

            Formpanel.Controls.Clear();
            ProjectsLauncherMain anamenu = new ProjectsLauncherMain();
            anamenu.Dock = DockStyle.Fill;
            anamenu.TopLevel = false;
            anamenu.FormBorderStyle = FormBorderStyle.None;
            Formpanel.Controls.Add(anamenu);
            anamenu.Show();

            nickname1.Text = ProjectsLauncherLogin.nickname;

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
        ///////////////
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
            serverstatuss.Text = server;
        }
        private void timer2_Tick(object sender, EventArgs e)
        {
            foreach (var process in Process.GetProcessesByName("javaw"))
            {
                surumchangelogs.Text = "Başlatılıyor...";
                surumchangelogs.Enabled = false;
                this.Visible = false;
            }


            if (!Process.GetProcessesByName("javaw").Any())
            {
                surumchangelogs.Text = "Oyna";
                surumchangelogs.Enabled = true;
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
        private void timer2_Tick_1(object sender, EventArgs e)
        {
            foreach (var process in Process.GetProcessesByName("javaw"))
            {
                surumchangelogs.Text = "Başlatılıyor...";
                surumchangelogs.Enabled = false;
                this.Visible = false;
            }


            if (!Process.GetProcessesByName("javaw").Any())
            {
                surumchangelogs.Text = "Oyna";
                surumchangelogs.Enabled = true;
                this.Visible = true;
            }
        }

        private void surumchangelogs_Click(object sender, EventArgs e)
        {
            int x = rnd.Next(255);
            int y = rnd.Next(255);
            int z = rnd.Next(255);
            anamenü.ForeColor = System.Drawing.Color.FromArgb(250, 235, 245);
            ayarlarbutton.ForeColor = System.Drawing.Color.FromArgb(250, 235, 245);
            

            surumchangelogs.ForeColor = System.Drawing.Color.FromArgb(x, y, z);

            Formpanel.Controls.Clear();
            Changelogs changelogs = new Changelogs();
            changelogs.Dock = DockStyle.Fill;
            changelogs.TopLevel = false;
            changelogs.FormBorderStyle = FormBorderStyle.None;
            Formpanel.Controls.Add(changelogs);
            changelogs.Show();
        }



        private void ayarlarbutton_Click(object sender, EventArgs e)
        {
            int x = rnd.Next(255);
            int y = rnd.Next(255);
            int z = rnd.Next(255);

            anamenü.ForeColor = System.Drawing.Color.FromArgb(250, 235, 245);
            surumchangelogs.ForeColor = System.Drawing.Color.FromArgb(250, 235, 245);
            ayarlarbutton.ForeColor = System.Drawing.Color.FromArgb(x, y, z);

            Formpanel.Controls.Clear();
            ProjectsLauncherOptions options = new ProjectsLauncherOptions();
            options.Dock = DockStyle.Fill;
            options.TopLevel = false;
            options.FormBorderStyle = FormBorderStyle.None;
            Formpanel.Controls.Add(options);
            options.Show();
        }
        private void anamenü_Click(object sender, EventArgs e)
        {
            int x = rnd.Next(255);
            int y = rnd.Next(255);
            int z = rnd.Next(255);
            ayarlarbutton.ForeColor = System.Drawing.Color.FromArgb(250, 235, 245);
            surumchangelogs.ForeColor = System.Drawing.Color.FromArgb(250, 235, 245);
            anamenü.ForeColor = System.Drawing.Color.FromArgb(x, y, z);

            Formpanel.Controls.Clear();
            ProjectsLauncherMain anamenu = new ProjectsLauncherMain();
            anamenu.Dock = DockStyle.Fill;
            anamenu.TopLevel = false;
            anamenu.FormBorderStyle = FormBorderStyle.None;
            Formpanel.Controls.Add(anamenu);
            anamenu.Show();
        }

        private void guna2ControlBox1_Click(object sender, EventArgs e)
        {
            Application.Exit();
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

        private void surumtext_Click(object sender, EventArgs e)
        {
            surumtextt = surumtext.Text;
        }
    }
}
