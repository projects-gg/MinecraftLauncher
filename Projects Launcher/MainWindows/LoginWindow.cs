using DiscordRPC;
using System;
using System.ComponentModel;
using System.Drawing;
using System.IO;
using System.Net;
using System.Text;
using System.Windows.Forms;

namespace Projects_Launcher
{
    public partial class loginMenuForm : Form //NOSONAR
    {
        public loginMenuForm()
        {
            InitializeComponent();
        }

        private readonly string currentVersion = "0";
        readonly Uri uri = new Uri("https://mc.projects.gg/LauncherUpdateStream/versions/ProjectsSetup.exe");

        public DiscordRpcClient Client { get; private set; }

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
                var request = WebRequest.Create("https://mc.projects.gg/LauncherUpdateStream/backgrounds" + "/" + random.Next(10) + ".png"); // Last background image

                using (var response = request.GetResponse())
                using (var stream = response.GetResponseStream())
                    this.BackgroundImage = Bitmap.FromStream(stream);
            }
            catch
            {
                // Shouldn't happen except no internet connection or server downtime
            }
        }

        private void cantGrabVersionInfo()
        {
            MessageBox.Show(
                "Güncelleme bilgileri alınamadı!\n\nİnternete bağlı olmayabilirsiniz ya da Projects servislerinde bir kara delik açılmış olabilir.",
                "Bilgi", MessageBoxButtons.OK, MessageBoxIcon.Information, MessageBoxDefaultButton.Button1);
        }

        private void ProjectsLauncherLogin_Load(object sender, EventArgs e)
        {
            DiscordRpcClientSetup();

            nickNameEnterTextBox.Text = Properties.Settings.Default.NickNames;

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
                if (!currentVersion.Equals(newestVersion))
                {
                    DialogResult updateDecision = MessageBox.Show(
                        "Projects başlatıcısı için kullanıma\nhazır yeni sürüm yayınlanmış!\n\n" +
                        $@"Güncel sürüm: {newestVersion}" + "\n" + $@"Sizin sürümünüz: {currentVersion}" + "\n" + "" +
                        "\n" + "Yeni sürüme güncellensin mi?", "Güncelleme Mevcut",
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

            selectBackgroundImage();
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
            if (rememberMeCheckBox.Checked == true)
            {
                Properties.Settings.Default.NickNames = nickNameEnterTextBox.Text;
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

           //Projects_Launcher.mainMenuForm.loginlocation = this.Location;
            //Projects_Launcher.mainMenuForm.loginresize = this.Size;
            Projects_Launcher.mainMenuForm main = new Projects_Launcher.mainMenuForm();

            Hide();

            main.Show();

            Client.Dispose();
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
