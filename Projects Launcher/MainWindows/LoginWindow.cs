﻿using DiscordRPC;
using System;
using System.ComponentModel;
using System.Drawing;
using System.IO;
using System.Net;
using System.Text;
using System.Windows.Forms;

namespace Projects_Launcher
{
    public partial class loginMenuForm : Form
    {
        public loginMenuForm()
        {
            InitializeComponent();
        }

        private readonly string currentVersion = Properties.Settings.Default.currentVersion;
        private string _newestVersion = "";
        private string newsTexts = "";
        private readonly Uri _setupLocation = new Uri("https://projects.gg/MinecraftLauncher/versions/ProjectsSetup.exe");

        public DiscordRpcClient Client { get; private set; }

        readonly Random rnd = new Random();
        private Color RandomColor()
        {
            return Color.FromArgb(rnd.Next(255), rnd.Next(255), rnd.Next(255));
        }
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
                        LargeImageKey = "projects_logo",
                        LargeImageText = "https://mc.projects.gg/",
                        SmallImageKey = "world",
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

                var request = WebRequest.Create("https://projects.gg/MinecraftLauncher/backgrounds" + "/" + imageType + ".png"); // Last background image

                using (var response = request.GetResponse())
                using (var stream = response.GetResponseStream())
                    this.BackgroundImage = Bitmap.FromStream(stream);
            }
            catch
            {
                // Shouldn't happen except no internet connection or server downtime
                this.BackgroundImage = Properties.Resources._6;
            }
        }

        private void newsTextsRead()
        {
            WebRequest newsText = HttpWebRequest.Create("https://projects.gg/MinecraftLauncher/yenilikler.php");
            try
            {
                WebResponse newsContentResponse;
                newsContentResponse = newsText.GetResponse();
                StreamReader versionContentReader = new StreamReader(newsContentResponse.GetResponseStream());
                string versionContentLine = versionContentReader.ReadToEnd();
                bool startWriting = false;
                StringBuilder blds = new StringBuilder();

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
                            blds.Append(character);
                        }
                        else
                        {
                            break;
                        }
                    }
                }

                if (blds.Length >= 0)
                {
                    newsTexts = blds.ToString();
                }
            }
            catch
            {
                newsLabel.Visible = false;
            }

            labelYenilikMaddeler.Text = newsTexts;
        }
        private void ProjectsLauncherLogin_Load(object sender, EventArgs e)
        {
            DiscordRpcClientSetup();

            versionLabel.Text = "v" + currentVersion;

            WebRequest currentVersionContent = HttpWebRequest.Create("https://projects.gg/MinecraftLauncher/version.php");

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
                    _newestVersion = bld.ToString();
                }
            }
            catch
            {
                cantGrabVersionInfo();
            }

            if (_newestVersion.Equals(""))
            {
                _newestVersion = currentVersion;
            }

            if (!currentVersion.Equals(_newestVersion))
            {
                if (Properties.Settings.Default.suppressVersion != _newestVersion)
                {
                    newVersionPanel.Visible = true;
                    vCurrentLabel.Text = currentVersion;
                    vLatestLabel.Text = _newestVersion;
                }
                else
                {
                    updateNowButton.Visible = true;
                }
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

            newsTextsRead();


            selectBackgroundImage();
            GC.WaitForPendingFinalizers();
        }

        private void Wc_DownloadFileCompleted(object sender, AsyncCompletedEventArgs e)
        {
            this.Hide();
            GC.SuppressFinalize(this);

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

        private void updateApplication()
        {
            this.Enabled = false;
            WebClient wc = new WebClient();
            wc.DownloadFileCompleted += Wc_DownloadFileCompleted;
            wc.DownloadFileAsync(_setupLocation,
                Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) +
                "/.projects/ProjectsSetup.exe");
        }

        private void button1_Click(object sender, EventArgs e)
        {
            updateApplication();
        }

        private void button2_Click(object sender, EventArgs e)
        {
            newVersionPanel.Visible = false;
        }

        private void button3_Click(object sender, EventArgs e)
        {
            DialogResult updateDecision = MessageBox.Show(
                "Bunun seçilmesi durumunda yeni bir\nsürüm yayınlanana kadar tekrar\ngüncelleme sorulmayacaktır.\n\nEmin misiniz?",
                "Güncelleme İptali", MessageBoxButtons.YesNo);

            if (updateDecision == DialogResult.Yes)
            {
                Properties.Settings.Default.suppressVersion = _newestVersion;
                Properties.Settings.Default.Save();
                newVersionPanel.Visible = false;
            }
        }

        private void updateNowButton_Click(object sender, EventArgs e)
        {
            updateApplication();
        }

        private void newsLabel_MouseEnter(object sender, EventArgs e)
        {
            newsLabel.ForeColor = RandomColor();
        }

        private void newsLabel_MouseLeave(object sender, EventArgs e)
        {
            newsLabel.ForeColor = Color.FromArgb(245, 245, 245);
        }

        private void backButton_Click(object sender, EventArgs e)
        {
            panelYenilikler.Visible = false;
            backButton.Visible = false;
        }

        private void newsLabel_Click(object sender, EventArgs e)
        {
            if (panelYenilikler.Visible == false)
            {
                backButton.Visible = true;
                panelYenilikler.Visible = true;
            }
            else
            {
                panelYenilikler.Visible = false;
            }
            GC.Collect();
            GC.WaitForPendingFinalizers();
        }
    }
}
