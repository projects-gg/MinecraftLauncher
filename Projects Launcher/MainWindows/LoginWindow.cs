using DiscordRPC;
using System;
using System.ComponentModel;
using System.Drawing;
using System.IO;
using System.Net;
using System.Text;
using System.Threading;
using System.Windows.Forms;

namespace Projects_Launcher
{
    public partial class loginMenuForm : Form
    {
        public loginMenuForm()
        {
            InitializeComponent();
        }

        public readonly string currentVersion = Properties.Settings.Default.currentVersion;
        readonly Uri uri = new Uri("https://mc.projects.gg/LauncherUpdateStream/versions/ProjectsSetup.exe");

        public DiscordRpcClient Client { get; private set; }

        private void cantGrabVersionInfo()
        {
            MessageBox.Show(
                "Güncelleme bilgileri alınamadı!\n\nİnternete bağlı olmayabilirsiniz ya da Projects servislerinde bir kara delik açılmış olabilir.",
                "Bilgi", MessageBoxButtons.OK, MessageBoxIcon.Information, MessageBoxDefaultButton.Button1);
        }
        public DiscordRpcClient GetClient()
        {
            return Client;
        }

        public void SetClient(DiscordRpcClient value)
        {
            Client = value;
        }

        public void DiscordRpcClientSetup()
        {
            try
            {
                Client = new DiscordRpcClient("949311557542756362");
                Client.Initialize();

                Client.SetPresence(new RichPresence
                {
                    Details = "Giriş ekranında",
                    State = "Sunucu IP: mc.projects.gg",
                    Assets = new Assets
                    {
                        LargeImageKey = "131231",
                        LargeImageText = "https://mc.projects.gg/",
                        SmallImageKey = "",
                    }
                });
            }
            catch
            {
                // Shouldn't happen except no internet connection or server downtime
            }
        }

        public void selectBackgroundImage()
        {
            // Grab background image
            try
            {
                var random = new Random();
                string imageType;

                if (Properties.Settings.Default.backgroundLite) // Need ternary support instead of this
                {
                    imageType = "lite";
                }
                else
                {
                    imageType = Convert.ToString(random.Next(4));
                }

                var request = WebRequest.Create("https://mc.projects.gg/LauncherUpdateStream/backgrounds" + "/" + imageType + ".png"); // Last background image
                
                using (var response = request.GetResponse())
                using (var stream = response.GetResponseStream())
                    this.BackgroundImage = Bitmap.FromStream(stream);
            }
            catch
            {
                // Shouldn't happen except no internet connection or server downtime
                this.BackgroundImage = Properties.Resources.defaultBg;
            }
        }

        private void ProjectsLauncherLogin_Load(object sender, EventArgs e)
        {
            this.Size = new Size(460, 200);
            this.MaximumSize = new Size(460, 200);

            updatePanel.Size = new Size(460, 200);
            updatePanel.MaximumSize = new Size(460, 200);
            DiscordRpcClientSetup();

            WebRequest currentVersionContent = HttpWebRequest.Create("https://mc.projects.gg/LauncherUpdateStream/version.php");

            string newestVersion = "";

            try
            {
                WebResponse versionContentResponse;
                versionContentResponse = currentVersionContent.GetResponse();
                StreamReader versionContentReader = new StreamReader(versionContentResponse.GetResponseStream());
                string versionContentLine = versionContentReader.ReadToEnd();
                bool startWriting = false;
                StringBuilder bld = new StringBuilder();

                foreach (char character in versionContentLine) //this is hard to read but culture-compatible
                {
                    if (character.Equals('>'))
                    {
                        if (!startWriting)
                        {
                            startWriting = true;
                        }
                    }
                    else if (startWriting)
                    {
                        if (!character.Equals('<'))
                        {
                            bld.Append(character);
                        }
                        else
                        {
                            break;
                        }
                    }
                }

                if (bld.Length >= 0)
                {
                    newestVersion = bld.ToString();
                }
            }
            catch
            {
                cantGrabVersionInfo();
            }

            if (newestVersion.Equals(""))
            {
                newestVersion = currentVersion;
            }

            try
            {
                if (currentVersion.Equals(newestVersion))
                {
                    newVersionLabel.Visible = false;
                    updateInfoLabel.Text = "Güncelleme Bulunamadı!";
                    Thread.Sleep(1000);


                    updatePanel.Visible = false;
                }
                else
                {
                    updatePanel.Visible = true;
                    updateInfoLabel.Text = "Yeni versiyon indiriliyor...";
                    newVersionLabel.Visible = true;
                    newVersionLabel.Text = $@"{currentVersion} => {newestVersion}";
                    this.Enabled = true;
                    WebClient wc = new WebClient();
                    wc.DownloadFileCompleted += Wc_DownloadFileCompleted;
                    wc.DownloadFileAsync(uri,
                        Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                        "/.projects/ProjectsSetup.exe");
                }
            }
            catch
            {
                cantGrabVersionInfo();
            }

            try
            {
                // Create new ".projects" directory if not exist
                if (!Directory.Exists(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                                      "/.projects"))
                {
                    Directory.CreateDirectory(@Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects");
                }
            }
            catch
            {
                // Shouldn't happen except no internet connection or server downtime
            }

            nickNameEnterTextBox.Text = Properties.Settings.Default.NickNames;

            selectBackgroundImage();
            GC.Collect();
            GC.WaitForPendingFinalizers();
        }

        private void Wc_DownloadFileCompleted(object sender, AsyncCompletedEventArgs e)
        {
            this.Hide();
            this.Size = new Size(977, 500);
            this.MaximumSize = new Size(977, 500);

            updatePanel.Size = new Size(977, 500);
            updatePanel.MaximumSize = new Size(977, 500);
            GC.SuppressFinalize(this);

            GC.Collect();
            GC.WaitForPendingFinalizers();

            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/ProjectsSetup.exe";

            string myPath = @appDataDizini;
            System.Diagnostics.Process prc = new System.Diagnostics.Process();
            prc.StartInfo.FileName = myPath;
            System.Threading.Thread.Sleep(1000);
            prc.Start();
            Environment.Exit(0);
        }
        private void benihatırla_CheckedChanged(object sender, EventArgs e)
        {
            if (rememberMeCheckBox.Checked == true)
            {
                Properties.Settings.Default.NickNames = nickNameEnterTextBox.Text;
                Properties.Settings.Default.Save();
            }
        }

        private void label3_Click(object sender, EventArgs e)
        {
            System.Diagnostics.Process.Start("https://mc.projects.gg/");
            GC.Collect();
            GC.WaitForPendingFinalizers();
        }

        private void guna2ControlBox1_Click(object sender, EventArgs e)
        {
            Application.Exit();
        }

        private void girisyapbutton_Click(object sender, EventArgs e)
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
            
            Projects_Launcher.mainMenuForm main = new Projects_Launcher.mainMenuForm();

            this.Hide();

            main.Show();

            GC.SuppressFinalize(this);

            this.BackgroundImage.Dispose();

            Client.Dispose();

            GC.Collect();
            GC.WaitForPendingFinalizers();
        }

        private void nicknametextbox_TextChanged(object sender, EventArgs e)
        {
            nickNameEnterTextBox.Text = (nickNameEnterTextBox.Text).Trim();

            if (!string.IsNullOrEmpty(nickNameEnterTextBox.Text))
            {
                if (rememberMeCheckBox.Checked == true)
                {
                    Properties.Settings.Default.NickNames = nickNameEnterTextBox.Text;
                    Properties.Settings.Default.Save();
                }

                loginButton.Text = "Giriş Yap";
                loginButton.Enabled = true;
            }
            else
            {
                loginButton.Text = "Kullanıcı Adı Giriniz";
                loginButton.Enabled = false;
            }
        }

        private void guna2ControlBox2_Click(object sender, EventArgs e)
        {
            this.WindowState = FormWindowState.Minimized;
        }
    }
}
