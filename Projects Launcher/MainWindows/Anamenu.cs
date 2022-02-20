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
        public static string rambox;
        public static string widthbox;
        public static string heightbox;
        public static string sayac;

        public static string ramlabell;
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

            if (Properties.Settings.Default.RamMBS != string.Empty)
            {
                ramtextbox.Text = Properties.Settings.Default.RamMBS;
            }
            ramtextbox.MaxLength = 4;

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

            var request = WebRequest.Create("https://minotar.net/helm/" + "/" + nickname1.Text);

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
                MaximumRamMb = int.Parse(Properties.Settings.Default.RamMBS),
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
            if (formpanel.Visible == false)
            {
                geriformpanel.Visible = true;
                formpanel.Visible = true;
            }
            else
            {
                formpanel.Visible = false;
            }

           
        }

        private void timer2_Tick(object sender, EventArgs e)
        {
            pingsayac++;

            string a, b, c;
            PingReply pr = p.Send("mc.projects.gg");
            a = pr.Status.ToString();
            b = pr.Address.ToString();
            c = pr.RoundtripTime.ToString();
            pingsayacc.Text = string.Format("{2} ms", a, b, c);
        }

        private void guna2ControlBox1_Click(object sender, EventArgs e)
        {
            Environment.Exit(0);
        }

        private void ramlabel_Click(object sender, EventArgs e)
        {
            ramlabell = ramlabel.Text;
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
            formpanell = formpanel.Visible;
        }

        private void geriformpanel_Click(object sender, EventArgs e)
        {
            formpanel.Visible = false;
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

        private void ramtextbox_TextChanged(object sender, EventArgs e)
        {
            rambox = ramtextbox.Text;
            Properties.Settings.Default.RamMBS = rambox;
            Properties.Settings.Default.Save();
            MainWindows.Anamenu.ramlabell = Properties.Settings.Default.RamMBS;
        }

        private void guna2PictureBox2_Click(object sender, EventArgs e)
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
            surumlabell = Properties.Settings.Default.SelectedVersion;
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
    }
}