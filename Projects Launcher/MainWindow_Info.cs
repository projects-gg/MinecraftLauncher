using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Data;
using System.Drawing;
using System.Linq;
using System.Diagnostics;
using System.IO;
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

        MSession session;
        string launcherdizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects";
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
            var proces = await launcher.CreateProcessAsync(surumtext.Text, ayarlar);

            proces.Start();
        }
    }
}
