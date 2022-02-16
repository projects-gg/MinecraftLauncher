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
        private void guna2Panel1_Paint(object sender, PaintEventArgs e)
        {

        }

        private void guna2PictureBox2_Click(object sender, EventArgs e)
        {

        }

        private void widthlabel_Click(object sender, EventArgs e)
        {

        }

        private void indirmeler_Click(object sender, EventArgs e)
        {

        }

        private void ramlabel_Click(object sender, EventArgs e)
        {

        }

        private void bilgibutton_Click(object sender, EventArgs e)
        {

        }

        private void pictureBox1_Click(object sender, EventArgs e)
        {

        }

        private void guna2ControlBox1_Click(object sender, EventArgs e)
        {

        }

        private void surumtext_Click(object sender, EventArgs e)
        {

        }

        private void nickname1_Click(object sender, EventArgs e)
        {

        }

        private void heightlabel_Click(object sender, EventArgs e)
        {

        }

        private void anamenü_Click(object sender, EventArgs e)
        {

        }

        private void ayarlarbutton_Click(object sender, EventArgs e)
        {

        }

        private void oynabutton_Click(object sender, EventArgs e)
        {

        }

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
    }
}
