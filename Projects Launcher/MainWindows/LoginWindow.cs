using System;
using System.ComponentModel;
using System.IO;
using System.Net;
using System.Windows.Forms;


namespace Projects_Launcher
{
    public partial class ProjectsLauncherLogin : Form
    {
        public ProjectsLauncherLogin()
        {
            InitializeComponent();
        }
        public static string nickname;
        public int v = 2;
        Uri setup = new Uri("https://mc.projects.gg/LauncherUpdateStream/versions/setup.exe");
        private void ProjectsLauncherLogin_Load(object sender, EventArgs e)
        {
            if (Properties.Settings.Default.NickNames != string.Empty)
            {
                nicknametextbox.Text = Properties.Settings.Default.NickNames;
            }

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

            if (v == 2)
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
                    wc.DownloadFileAsync(setup,
                        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/setup.exe");
                }
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
            MainWindows.Anamenu main = new MainWindows.Anamenu();
            this.Hide();
            main.Show();
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
                return;
            }
            else
            {
                girisyapbutton.Text = "Kullanıcı Adı Giriniz";
            }
        }

        private void guna2Button1_Click(object sender, EventArgs e)
        {
            string zipPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/mods.zip";
            string extractPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft";
        }
    }
}
