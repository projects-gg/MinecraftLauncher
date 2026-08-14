using DiscordRPC;
using Microsoft.Win32;
using Projects_Launcher.Auth;
using System;
using System.ComponentModel;
using System.Drawing;
using System.IO;
using System.Net;
using System.Text;
using System.Windows.Forms;
using System.Text.RegularExpressions;
using System.Threading.Tasks;

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
        private readonly Uri _setupLocation = new Uri("https://mc.projects.gg/LauncherUpdateStream/versions/ProjectsSetup.exe");

        public DiscordRpcClient Client { get; private set; }

        // Açılan ana menü saklanır: "hesap değiştir" ile buraya dönülüp tekrar girildiğinde aynı
        // pencere gösterilir. Yeni bir ana menü açmak Discord bağlantısını, zamanlayıcıları ve
        // AFK panelini ikiye katlardı.
        private Projects_Launcher.mainMenuForm _mainMenu;
        
        private void cantGrabVersionInfo()
        {
            MessageBox.Show(
                "Güncelleme bilgileri alınamadı!\n\nİnternete bağlı olmayabilirsiniz ya da Projects servislerinde bir kara delik açılmış olabilir.",
                "Bilgi", MessageBoxButtons.OK, MessageBoxIcon.Information, MessageBoxDefaultButton.Button1);
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
                    imageType = Convert.ToString(random.Next(9) + 1);
                }

                var request = WebRequest.Create("https://mc.projects.gg/LauncherUpdateStream/backgrounds" + "/" + imageType + ".png"); // Last background image

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

        // Yenilikleri çeker (UI'ye dokunmaz; arka planda çağrılabilsin diye string döner).
        private string FetchNewsText()
        {
            try
            {
                WebRequest newsText = HttpWebRequest.Create("https://mc.projects.gg/LauncherUpdateStream/yenilikler.php");
                WebResponse newsContentResponse = newsText.GetResponse();
                StreamReader versionContentReader = new StreamReader(newsContentResponse.GetResponseStream());
                string versionContentLine = versionContentReader.ReadToEnd();
                bool startWriting = false;
                StringBuilder blds = new StringBuilder();

                foreach (char character in versionContentLine)
                {
                    if (character.Equals('>'))
                    {
                        if (!startWriting)
                            startWriting = true;
                    }
                    else if (startWriting)
                    {
                        if (!character.Equals('<'))
                            blds.Append(character);
                        else
                            break;
                    }
                }

                return blds.ToString();
            }
            catch
            {
                return "Yenilik bilgileri alınamadı.";
            }
        }

        public String readPhpContent(String address)
        {
            try
            {

                WebRequest currentVersionContent = HttpWebRequest.Create(address);
                WebResponse versionContentResponse = currentVersionContent.GetResponse();
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

                return _newestVersion;
            }
            catch
            {
                return ""; // hata: Load varsayılana düşer, kullanıcı bloklanmaz
            }
        }

        private async void ProjectsLauncherLogin_Load(object sender, EventArgs e)
        {
            DiscordRpcClientSetup();

            versionLabel.Text = "v" + currentVersion;
            nickNameEnterTextBox.Text = Properties.Settings.Default.NickNames;

            ApplyPremiumAvailability();

            // ".projects" dizinini oluştur (yoksa)
            try
            {
                Directory.CreateDirectory(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects");
            }
            catch
            {
                // kritik değil
            }

            // Tema (senkron, hızlı). Pencere simgesi temaya göre değişmez; her zaman projects.ico'dur.
            if (Properties.Settings.Default.themeSelected == "Sistem Varsayılanı")
            {
                int res = (int)Registry.GetValue("HKEY_CURRENT_USER\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize", "AppsUseLightTheme", -1);
                if (res == 1)
                    this.BackgroundImage = Properties.Resources.gaia_light;
                if (res == 0)
                    this.BackgroundImage = Properties.Resources.gaia_dark;
            }

            if (Properties.Settings.Default.themeSelected == "Açık Tema")
                this.BackgroundImage = Properties.Resources.gaia_light;

            if (Properties.Settings.Default.themeSelected == "Koyu Tema")
                this.BackgroundImage = Properties.Resources.gaia_dark;

            // --- Ağ işleri arka planda; giriş ekranı anında açılır (eskiden senkron HTTP donduruyordu) ---
            string latest = await Task.Run(() => readPhpContent("https://mc.projects.gg/LauncherUpdateStream/version-v2.php"));
            if (string.IsNullOrEmpty(latest))
                latest = currentVersion;
            _newestVersion = latest;

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

            labelYenilikMaddeler.Text = await Task.Run(() => FetchNewsText());
        }

        private void Wc_DownloadFileCompleted(object sender, AsyncCompletedEventArgs e)
        {
            this.Hide();

            string setupPath = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/ProjectsSetup.exe";
            System.Diagnostics.Process.Start(setupPath); // indirilen güncelleyiciyi çalıştır
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
            } else if (!Regex.IsMatch(nickNameEnterTextBox.Text, "^[a-zA-Z0-9_]*$"))
            {
                // Need invalid nickname image instead of MessageBox
                MessageBox.Show("Kullanıcı adınızda boşluk veya ? gibi\nözel karakterler bulunmamalıdır!", "Kullanıcı Adı Geçersiz");
                return;
            }

            // Kayıtlı premium hesap varken kullanıcı adıyla girmek çelişkilidir: oyun kimliği
            // premium oturuma göre belirlenir ve girilen ad yok sayılırdı. Kullanıcı onaylarsa
            // premium oturum kapatılır ve oyuna çevrimdışı girilir.
            if (PremiumSession.IsActive)
            {
                DialogResult offlineDecision = MessageBox.Show(
                    "Kayıtlı premium hesabınız var: " + PremiumSession.ActiveName + "\n\n" +
                    "Kullanıcı adıyla girerseniz premium oturum kapatılacak ve oyuna çevrimdışı gireceksiniz.\n\nDevam edilsin mi?",
                    "Premium Hesap", MessageBoxButtons.YesNo, MessageBoxIcon.Question);

                if (offlineDecision != DialogResult.Yes)
                    return;

                PremiumSession.SignOut();
                RefreshPremiumButton();
            }

            OpenMainMenu();
        }

        // --- Premium (orijinal hesap) girişi ---

        // Microsoft logosu kod içinde çizilir; bir kez üretilip yeniden kullanılır.
        private void EnsurePremiumButtonImage()
        {
            if (premiumLoginButton.Image == null)
                premiumLoginButton.Image = MicrosoftBrand.CreateLogo(18);
        }

        /// <summary>
        /// Premium giriş kapatıldığında ilgili tüm öğeler gizlenir ve site bağlantısı, bu blok hiç
        /// yokmuş gibi eski yerine (giriş butonunun hemen altına) çekilir; ekranda boşluk kalmasın.
        /// </summary>
        private void ApplyPremiumAvailability()
        {
            if (PremiumSession.IsEnabled)
            {
                EnsurePremiumButtonImage();
                RefreshPremiumButton();
                return;
            }

            loginSeparatorLabel.Visible = false;
            premiumLoginButton.Visible = false;
            premiumHintLabel.Visible = false;

            webbutton.Location = new Point(webbutton.Left, loginButton.Bottom + 6);
        }

        // Kayıtlı bir premium oturum varsa buton yeniden giriş istemek yerine devam etmeyi önerir.
        private void RefreshPremiumButton()
        {
            PremiumAccount account = PremiumSession.Current;

            if (account == null)
            {
                premiumLoginButton.Text = "Premium Giriş";
                premiumHintLabel.Text = "Satın alınmış Minecraft hesabı gerekir";
                return;
            }

            premiumLoginButton.Text = account.Name + " ile devam et";
            premiumHintLabel.Text = "Premium hesabınız kayıtlı";
        }

        private void premiumLoginButton_Click(object sender, EventArgs e)
        {
            PremiumAccount account = PremiumSession.Current;

            if (account == null)
            {
                account = PremiumLoginForm.Prompt(this);

                if (account == null)
                    return; // kullanıcı vazgeçti ya da giriş tamamlanamadı
            }

            // Ana menü oyuncu adını ve deri görselini ayarlardan okur; premium ad oraya yazılır.
            Properties.Settings.Default.NickNames = account.Name;
            Properties.Settings.Default.Save();

            nickNameEnterTextBox.Text = account.Name;

            OpenMainMenu();
        }

        // Giriş ekranından ana menüye geçiş (normal ve premium giriş için ortak).
        private void OpenMainMenu()
        {
            if (_mainMenu != null && !_mainMenu.IsDisposed)
                _mainMenu.OnReturnedFromLogin(); // "hesap değiştir" ile buraya gelinmişti
            else
                _mainMenu = new Projects_Launcher.mainMenuForm(false, this);

            this.Hide();

            _mainMenu.Show();
            _mainMenu.BringToFront();
            _mainMenu.Activate();

            // Discord durumu ana menüde yeniden kurulur; giriş ekranınınki serbest bırakılır.
            // Arka plan görseli bilerek korunur: hesap değiştirildiğinde bu ekran tekrar açılır.
            Client?.Dispose();
            Client = null;
        }

        /// <summary>
        /// Ana menüden giriş ekranına dönülürken çağrılır: dönülecek ana menü hatırlanır ve ekran
        /// güncel duruma göre tazelenir (kayıtlı ad, premium hesap, Discord durumu).
        /// </summary>
        public void PrepareForReturn(Projects_Launcher.mainMenuForm mainMenu)
        {
            _mainMenu = mainMenu;

            nickNameEnterTextBox.Text = Properties.Settings.Default.NickNames;

            ApplyPremiumAvailability();

            if (Client == null)
                DiscordRpcClientSetup();
        }

        protected override void OnFormClosed(FormClosedEventArgs e)
        {
            base.OnFormClosed(e);

            // Giriş ekranının kapanması her zaman "başlatıcıdan çık" demektir. Ana menü gizli
            // duruyor olabileceğinden süreç görünmez biçimde ayakta kalmasın; ona bağlı AFK alt
            // süreçleri de burada sonlandırılır.
            Afk.AfkManager.Instance.StopAll(5000);
            Environment.Exit(0);
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

                loginButton.Image = Properties.Resources.giris;
                loginButton.Enabled = true;
            }
            else
            {
                loginButton.Image = Properties.Resources.kullaniciadigiriniz;
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

        private void backButton_Click(object sender, EventArgs e)
        {
            panelYenilikler.Visible = false;
            backButton.Visible = false;
        }
        private void newsLabel_Click_1(object sender, EventArgs e)
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
        }

        private void webbutton_Click(object sender, EventArgs e)
        {
            System.Diagnostics.Process.Start("https://mc.projects.gg/");
        }
    }
}
