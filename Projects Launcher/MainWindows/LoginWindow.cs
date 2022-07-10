using DiscordRPC;
using System;
using System.IO;
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

        private void ProjectsLauncherLogin_Load(object sender, EventArgs e)
        {
            DiscordRpcClientSetup();

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

            GC.WaitForPendingFinalizers();
        }

        private void rememberMeCheckBox_CheckedChanged(object sender, EventArgs e)
        {
            if (rememberMeCheckBox.Checked == true)
            {
                Properties.Settings.Default.NickNames = nickNameEnterTextBox.Text;
                Properties.Settings.Default.Save();
            }
        }

        private void nickNameEnterTextBox_TextChanged(object sender, EventArgs e)
        {
            nickNameEnterTextBox.Text = (nickNameEnterTextBox.Text).Trim();

            if (!string.IsNullOrEmpty(nickNameEnterTextBox.Text))
            {
                if (rememberMeCheckBox.Checked == true)
                {
                    Properties.Settings.Default.NickNames = nickNameEnterTextBox.Text;
                    Properties.Settings.Default.Save();
                }
                loginButton.Enabled = true;
            }
            else
            {
                ;
                loginButton.Enabled = false;
            }
        }

        private void loginButton_Click(object sender, EventArgs e)
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

            //this.BackgroundImage.Dispose();

            Client.Dispose();

            GC.WaitForPendingFinalizers();
            this.BackgroundImage = null;
        }

        private void webbutton_Click(object sender, EventArgs e)
        {
            System.Diagnostics.Process.Start("https://mc.projects.gg/");
            GC.WaitForPendingFinalizers();
        }

        private void loginMenuForm_FormClosed(object sender, FormClosedEventArgs e)
        {
            Environment.Exit(0);
        }
    }
}
