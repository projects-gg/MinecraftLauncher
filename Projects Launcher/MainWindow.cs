﻿using System;
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
    public partial class ProjectsLauncherMain : Form
    {
        public ProjectsLauncherMain()
        {
            InitializeComponent();
            Control.CheckForIllegalCrossThreadCalls = false;
        }
        public static string versiyon;

        public static string sessions;
        MSession session;

        public static string rammiktar;
        public static string height;
        public static string width;
        public static string versiyons;

        Ping p = new Ping();

        int pingsayac;

        string launcherdizin = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects";
        private void ProjectsLauncherMain_Load(object sender, EventArgs e)
        {
            //log.ReadOnly = true;
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

        private async Task ServerStatus()
        {
            IMinecraftPinger pinger = new MinecraftPinger("193.164.7.43", 25565);
            var status = await pinger.RequestAsync();
            String server = status.Players.Online + "";
            serverstatus.Text = server;
        }

        private void oynabutton_Click(object sender, EventArgs e)
        {
            string fabric_appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/versions/" + Properties.Settings.Default.SelectedVersion;
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
            Uri fabric = new Uri("https://www.dropbox.com/s/agaj6ootu3cmvok/fabric-installer-0.10.2.jar?dl=1");

            if (Directory.Exists(@fabric_appDataDizini))
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
                    wc.DownloadFileAsync(fabric, appDataDizini + "/.projects/fabric-installer-0.10.2.jar");
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
                oynabutton.Text = "Başlatılıyor...";
                oynabutton.Enabled = false;
                this.Enabled = false;
            }

            
            if (!Process.GetProcessesByName("javaw").Any())
            {
                oynabutton.Text = "Oyna";
                oynabutton.Enabled = true;
                this.Enabled = true;
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
            var clientStartProcess = await launcher.CreateProcessAsync(surumtext.Text, ayarlar);

            clientStartProcess.Start();

        }

        private void ayarlarbutton_Click(object sender, EventArgs e)
        {
            ProjectsLauncherOptions Options = new ProjectsLauncherOptions();
            this.Hide();
            Options.Show();
        }

        private void bilgibutton_Click(object sender, EventArgs e)
        {
            ProjectsLauncherInfo info = new ProjectsLauncherInfo();
            this.Hide();
            info.Show();
        }

        private void indirmeler_Click(object sender, EventArgs e)
        {
            ProjectsLauncherDownload Download = new ProjectsLauncherDownload();
            Download.Show();
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

        private void ayarlarbutton_MouseEnter(object sender, EventArgs e)
        {
            ayarlarbutton.ForeColor =  System.Drawing.Color.FromArgb(255,255,255);
        }

        private void ayarlarbutton_MouseLeave(object sender, EventArgs e)
        {
            ayarlarbutton.ForeColor = System.Drawing.Color.FromArgb(250,235,246);
        }

        private void bilgibutton_MouseLeave(object sender, EventArgs e)
        {
            bilgibutton.ForeColor = System.Drawing.Color.FromArgb(250, 235, 246);
        }

        private void bilgibutton_MouseEnter(object sender, EventArgs e)
        {
            bilgibutton.ForeColor = System.Drawing.Color.FromArgb(255,255,255);
        }

        private void indirmeler_MouseLeave(object sender, EventArgs e)
        {
            indirmeler.ForeColor = System.Drawing.Color.FromArgb(250, 235, 246);
        }

        private void indirmeler_MouseEnter(object sender, EventArgs e)
        {
            indirmeler.ForeColor = System.Drawing.Color.FromArgb(255,255,255);
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