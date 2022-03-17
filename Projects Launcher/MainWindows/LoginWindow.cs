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
    public partial class loginMenuForm : Form
    {
        public loginMenuForm()
        {
            InitializeComponent();
        }
        public static string nickname;
        public static int index;
        public string currentVersion = "0";
        Uri uri = new Uri("https://mc.projects.gg/LauncherUpdateStream/versions/setup.exe");

        public DiscordRpcClient Client { get; private set; }

        public void Setup()
        {
            try
            {
                Client = new DiscordRpcClient("949311557542756362");
                Client.Initialize();

                Client.SetPresence(new RichPresence()
                {
                    Details = "Giriş ekranında",
                    State = "Sunucu IP: mc.projects.gg",
                    Assets = new Assets()
                    {
                        LargeImageKey = "131231",
                        LargeImageText = "https://mc.projects.gg/",
                        SmallImageKey = "",

                    }
                });
            }
            catch
            {

            }

        }

        private void cantGrabVersionInfo()
        {
            MessageBox.Show("Güncelleme bilgileri alınamadı!\n\nİnternete bağlı olmayabilirsiniz ya da Projects servislerinde bir kara delik açılmış olabilir.", "Bilgi", MessageBoxButtons.OK, MessageBoxIcon.Information, MessageBoxDefaultButton.Button1);
            
        }

        private void ProjectsLauncherLogin_Load(object sender, EventArgs e)
        {
            Setup();
            nickNameEnterTextBox.Text = Properties.Settings.Default.NickNames;
            
            WebRequest currentVersionContent = HttpWebRequest.Create("https://mc.projects.gg/LauncherUpdateStream/version.php");

            string newestVersion = currentVersion;

            try
            {
                WebResponse versionContentResponse;
                versionContentResponse = currentVersionContent.GetResponse();
                StreamReader versionContentReader = new StreamReader(versionContentResponse.GetResponseStream());
                string versionContentLine = versionContentReader.ReadToEnd();
                int formatSplitterStart = versionContentLine.IndexOf("<p>") + 3;
                int formatSplitterEnd = versionContentLine.Substring(formatSplitterStart).IndexOf("</p>");
                newestVersion = versionContentLine.Substring(formatSplitterStart, formatSplitterEnd);
            }
            catch
            {
                cantGrabVersionInfo();
            }

            try
            {
                if (!currentVersion.Equals(newestVersion))
                {
                    DialogResult updateDecision = MessageBox.Show("Projects başlatıcısı için kullanıma\nhazır yeni sürüm yayınlanmış!\n\n" + $@"Güncel sürüm: {newestVersion}" + "\n" + $@"Sizin sürümünüz: {currentVersion}" + "\n" + "" + "\n" + "Yeni sürüme güncellensin mi?", "Güncelleme Mevcut",
                          MessageBoxButtons.YesNo, MessageBoxIcon.Information, MessageBoxDefaultButton.Button1);

                    if (updateDecision == DialogResult.Yes)
                    {
                        this.Enabled = false;
                        WebClient wc = new WebClient();
                        wc.DownloadFileCompleted += Wc_DownloadFileCompleted;
                        wc.DownloadFileAsync(uri,
                            Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                            "/.projects/ProjectsSetup.exe");
                    }
                }
            }
            catch
            {
                cantGrabVersionInfo();
            }

            try
            {
                // Create new ".projects" directory if not exist
                if (!Directory.Exists(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects"))
                {
                    Directory.CreateDirectory(@Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects");
                }
            }
            catch
            {

            }

            try
            {
                // Get background info
                var random = new Random();
                var BackgroundList = new List<string> { "kıs_meydan.png", "balık2.png", "kıs_meydan2.png", "maden.png", "maden2.png", "meydan.png", "world.png", "world2.png", "world3.png", "world4.png" };
                index = random.Next(BackgroundList.Count);

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

            try
            {
                string myPath = @appDataDizini;
                System.Diagnostics.Process prc = new System.Diagnostics.Process();
                prc.StartInfo.FileName = myPath;
                System.Threading.Thread.Sleep(1000);
                prc.Start();
                Environment.Exit(0);
            }
            catch
            {

            }
        }

        private void benihatırla_CheckedChanged(object sender, EventArgs e)
        {
            try
            {
                if (rememberMeCheckBox.Checked == true)
                {
                    Properties.Settings.Default.NickNames = nickNameEnterTextBox.Text;
                    Properties.Settings.Default.Save();
                }
            }
            catch
            {

            }
        }

        private void label3_Click(object sender, EventArgs e)
        {
            try
            {
                System.Diagnostics.Process.Start("https://mc.projects.gg/");
            }
            catch
            {

            }
        }

        private void guna2ControlBox1_Click(object sender, EventArgs e)
        {
            
            try
            {
                Application.Exit();
            }
            catch
            {

            }
        }

        private void girisyapbutton_Click(object sender, EventArgs e)
        {
            try
            {
                if (string.IsNullOrEmpty(nickNameEnterTextBox.Text))
                {
                    if (rememberMeCheckBox.Checked == true)
                    {
                        Properties.Settings.Default.NickNames = nickNameEnterTextBox.Text;
                        Properties.Settings.Default.Save();
                    }
                    loginButton.Text = "Kullanıcı Adı Giriniz";
                    return;
                }
                else
                {
                    nickname = nickNameEnterTextBox.Text;
                    loginButton.Text = "Giriş Yap";
                }
                Projects_Launcher.mainMenuForm main = new Projects_Launcher.mainMenuForm();
                this.Hide();
                main.Show(); Client.Dispose();
            }
            catch
            {
                MessageBox.Show("Giriş yapılırken bir hata meydana geldi.");
            }
        }

        private void nicknametextbox_TextChanged(object sender, EventArgs e)
        {
            nickNameEnterTextBox.Text = (nickNameEnterTextBox.Text).Trim();

            try
            {
                if (!string.IsNullOrEmpty(nickNameEnterTextBox.Text))
                {
                    if (rememberMeCheckBox.Checked == true)
                    {
                        Properties.Settings.Default.NickNames = nickNameEnterTextBox.Text;
                        Properties.Settings.Default.Save();
                    }
                    loginButton.Text = "Giriş Yap";
                    loginButton.Enabled = true;
                    return;
                }
                else
                {
                    loginButton.Text = "Kullanıcı Adı Giriniz";
                    loginButton.Enabled = false;
                }
            }
            catch
            {

            }
        }

        private void guna2ControlBox2_Click(object sender, EventArgs e)
        {
            try
            {
                this.WindowState = FormWindowState.Minimized;

            }
            catch
            {

            }
        }
    }
}
