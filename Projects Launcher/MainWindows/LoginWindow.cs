using DiscordRPC;
using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Drawing;
using System.IO;
using System.Net;
using System.Windows.Forms;

namespace Projects_Launcher
{
    public partial class ProjectsLauncherLogin : Form
    {
        public ProjectsLauncherLogin()
        {
            InitializeComponent();//
        }
        public static string nickname;
        public static int index;
        public int v = 2023;
        Uri uri = new Uri("https://mc.projects.gg/LauncherUpdateStream/versions/setup.exe");

        public DiscordRpcClient Client { get; private set; }

        public void Setup()
        {
            Client = new DiscordRpcClient("949311557542756362");  //Creates the client
            Client.Initialize();                            //Connects the client

            Client.SetPresence(new RichPresence()
            {
                Details = "Giriş Ekranında - Projects Survival",
                State = "Sunucu IP: mc.projects.gg",
                Assets = new Assets()
                {
                    LargeImageKey = "131231",
                    LargeImageText = "https://mc.projects.gg/",
                    SmallImageKey = "",

                }
            });
        }

        private void ProjectsLauncherLogin_Load(object sender, EventArgs e)
        {
            Setup();
            nicknametextbox.Text = Properties.Settings.Default.NickNames;

            string hedef = "https://mc.projects.gg/LauncherUpdateStream/version.php";
            WebRequest istek = HttpWebRequest.Create(hedef);
            WebResponse yanit;
            yanit = istek.GetResponse();
            StreamReader bilgiler = new StreamReader(yanit.GetResponseStream());
            string gelen = bilgiler.ReadToEnd();
            int baslangic = gelen.IndexOf("<p>") + 3;
            int bitis = gelen.Substring(baslangic).IndexOf("</p>");
            string gelenbilgileri = gelen.Substring(baslangic, bitis);
            v = Convert.ToInt16(gelenbilgileri);

            try
            {
                if (v == 2023)
                {

                }
                else
                {
                    DialogResult secenek = MessageBox.Show($@"Yeni Sürüm: {v}" + "\n" + "" + "\n" + "Yeni sürüme güncellensin mi?", "Güncelleme Mevcut",
                          MessageBoxButtons.YesNo, MessageBoxIcon.Information, MessageBoxDefaultButton.Button1);

                    if (secenek == DialogResult.Yes)
                    {
                        this.Enabled = false;
                        WebClient wc = new WebClient();
                        wc.DownloadFileCompleted += Wc_DownloadFileCompleted;
                        wc.DownloadFileAsync(uri,
                            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/setup.exe");
                    }
                }
            }
            catch
            {
                DialogResult secenek = MessageBox.Show($@"Güncelleme bilgileri alınamadı.", "Bilgi",
                          MessageBoxButtons.OK, MessageBoxIcon.Information, MessageBoxDefaultButton.Button1);

                if (secenek == DialogResult.OK)
                {

                }
            }

            //projects kontrol
            if (Directory.Exists(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects"))
            {

            }
            else
            {
                Directory.CreateDirectory(@Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects");
            }


            //Arkaplan bilgisini al
            var random = new Random();
            var BackgroundList = new List<string> { "kıs_meydan.png", "balık2.png", "kıs_meydan2.png", "maden.png", "maden2.png", "meydan.png", "world.png", "world2.png", "world3.png", "world4.png" };
            index = random.Next(BackgroundList.Count);


            try
            {
                var request = WebRequest.Create("https://mc.projects.gg/LauncherUpdateStream/background" + "/" + (BackgroundList[index]));

                using (var response = request.GetResponse())
                using (var stream = response.GetResponseStream())
                {
                    this.BackgroundImage = Bitmap.FromStream(stream);
                }
            }
            catch
            {

            }


        }

        private void Wc_DownloadFileCompleted(object sender, AsyncCompletedEventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/setup.exe";

            string myPath = @appDataDizini;
            System.Diagnostics.Process prc = new System.Diagnostics.Process();
            prc.StartInfo.FileName = myPath;
            System.Threading.Thread.Sleep(1000);
            prc.Start();
            Environment.Exit(0);


        }

        private void benihatırla_CheckedChanged(object sender, EventArgs e)
        {
            if (benihatırla.Checked == true)
            {
                Properties.Settings.Default.NickNames = nicknametextbox.Text;
                Properties.Settings.Default.Save();
            }
        }

        private void label3_Click(object sender, EventArgs e)
        {
            System.Diagnostics.Process.Start("https://mc.projects.gg/");
        }

        private void guna2ControlBox1_Click(object sender, EventArgs e)
        {
            Application.Exit();
        }

        private void girisyapbutton_Click(object sender, EventArgs e)
        {
            if (string.IsNullOrEmpty(nicknametextbox.Text))
            {
                if (benihatırla.Checked == true)
                {
                    Properties.Settings.Default.NickNames = nicknametextbox.Text;
                    Properties.Settings.Default.Save();
                }
                girisyapbutton.Text = "Kullanıcı Adı Giriniz";
                return;
            }
            else
            {
                nickname = nicknametextbox.Text;
                girisyapbutton.Text = "Giriş Yap";
            }
            Projects_Launcher.Anamenu main = new Projects_Launcher.Anamenu();
            this.Hide();
            main.Show(); Client.Dispose();
        }

        private void nicknametextbox_TextChanged(object sender, EventArgs e)
        {
            nicknametextbox.Text = (nicknametextbox.Text).Trim();
            if (!string.IsNullOrEmpty(nicknametextbox.Text))
            {
                if (benihatırla.Checked == true)
                {
                    Properties.Settings.Default.NickNames = nicknametextbox.Text;
                    Properties.Settings.Default.Save();
                }
                girisyapbutton.Text = "Giriş Yap";
                girisyapbutton.Enabled = true;
                return;
            }
            else
            {
                girisyapbutton.Text = "Kullanıcı Adı Giriniz";
                girisyapbutton.Enabled = false;
            }
        }

        private void guna2Button1_Click(object sender, EventArgs e)
        {
            string zipPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/mods.zip";
            string extractPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects";
        }
    }
}
