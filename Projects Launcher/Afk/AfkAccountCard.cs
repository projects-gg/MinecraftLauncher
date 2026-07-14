using System;
using System.Drawing;
using System.Threading.Tasks;
using System.Windows.Forms;
using Guna.UI2.WinForms;

namespace Projects_Launcher.Afk
{
    /// <summary>
    /// Listede tek bir AFK hesabını temsil eden kart: kafa görseli, kullanıcı adı, canlı durum ve
    /// bağlan/ayarlar/konsol/kaldır eylemleri.
    ///
    /// Kart, oturumun <see cref="AfkSession.Changed"/> olayına abone olur. Bu olay iş parçacığı
    /// havuzunda tetiklendiği için tazeleme her zaman pencerenin thread'ine marshal edilir.
    /// </summary>
    public class AfkAccountCard : Guna2Panel
    {
        private static readonly Color Background = Color.FromArgb(20, 22, 27);
        private static readonly Color Surface = Color.FromArgb(32, 36, 44);
        private static readonly Color BorderTone = Color.FromArgb(52, 58, 70);
        private static readonly Color TextTone = Color.FromArgb(245, 247, 250);
        private static readonly Color MutedTone = Color.FromArgb(152, 162, 179);
        private static readonly Color GreenTone = Color.FromArgb(34, 197, 94);
        private static readonly Color RedTone = Color.FromArgb(239, 68, 68);
        private static readonly Color AmberTone = Color.FromArgb(245, 158, 11);

        public const int CardWidth = 878;
        public const int CardHeight = 76;

        private readonly PictureBox avatarBox;
        private readonly Label nicknameLabel;
        private readonly Guna2Panel statusDot;
        private readonly Label statusLabel;
        private readonly Guna2Button connectButton;
        private readonly Guna2Button settingsButton;
        private readonly Guna2Button consoleButton;
        private readonly Guna2Button removeButton;

        private AfkSession session;

        // Kartın tüm simge butonları tek ToolTip bileşenini paylaşır; buton başına yeni ToolTip
        // üretmek her kartta dört adet kapatılamayan pencere tanıtıcısı sızdırıyordu.
        private readonly ToolTip iconTips = new ToolTip { UseFading = true, ShowAlways = true };

        // "Tümünü Bağla" sırayla bağlarken bu kartın bağlanmasına kalan saniye. null: geri sayım yok.
        private int? pendingCountdown;

        public AfkAccountCard(AfkAccount account)
        {
            if (account == null) throw new ArgumentNullException("account");
            Account = account;

            Size = new Size(CardWidth, CardHeight);
            Margin = new Padding(0, 0, 0, 10);
            BorderRadius = 10;
            FillColor = Surface;
            BackColor = Background; // yuvarlak köşeler liste zeminine karışsın (beyaz kalmasın)
            BorderColor = BorderTone;
            BorderThickness = 1;

            avatarBox = new PictureBox
            {
                Location = new Point(16, 16),
                Size = new Size(44, 44),
                SizeMode = PictureBoxSizeMode.StretchImage,
                BackColor = Color.Transparent,
                Image = AfkAvatar.Placeholder,
            };

            nicknameLabel = new Label
            {
                AutoSize = true,
                Location = new Point(74, 15),
                Font = new Font("Segoe UI", 11.25f, FontStyle.Bold),
                ForeColor = TextTone,
                BackColor = Color.Transparent,
                Text = account.Nickname,
            };

            statusDot = new Guna2Panel
            {
                Location = new Point(76, 45),
                Size = new Size(8, 8),
                BorderRadius = 4,
                FillColor = MutedTone,
                BackColor = Surface, // yuvarlak nokta köşeleri kart yüzeyine karışsın
            };

            statusLabel = new Label
            {
                AutoSize = true,
                Location = new Point(90, 41),
                Font = new Font("Segoe UI", 8.25f),
                ForeColor = MutedTone,
                BackColor = Color.Transparent,
                Text = "Bağlı değil",
            };

            removeButton = MakeIconButton("−", "Hesabı kaldır", RedTone);
            removeButton.Location = new Point(CardWidth - 16 - 36, 20);

            consoleButton = MakeIconButton(">_", "Konsolu aç", Color.FromArgb(52, 58, 70));
            consoleButton.Font = new Font("Consolas", 10f, FontStyle.Bold);
            consoleButton.Location = new Point(removeButton.Left - 8 - 36, 20);

            settingsButton = MakeIconButton("⚙", "AFK ayarları", Color.FromArgb(52, 58, 70));
            settingsButton.Location = new Point(consoleButton.Left - 8 - 36, 20);

            connectButton = new Guna2Button
            {
                Size = new Size(104, 36),
                Location = new Point(settingsButton.Left - 12 - 104, 20),
                BorderRadius = 8,
                FillColor = GreenTone,
                BackColor = Surface, // köşeler kart yüzeyine karışsın (beyaz kalmasın)
                ForeColor = Color.White,
                Font = new Font("Segoe UI", 9.5f, FontStyle.Bold),
                Cursor = Cursors.Hand,
                Text = "Bağla",
            };
            connectButton.HoverState.FillColor = Color.FromArgb(22, 163, 74);
            connectButton.DisabledState.FillColor = Color.FromArgb(60, 64, 72);
            connectButton.DisabledState.ForeColor = MutedTone;

            connectButton.Click += (s, e) => Raise(ToggleConnectionRequested);
            settingsButton.Click += (s, e) => Raise(SettingsRequested);
            consoleButton.Click += (s, e) => Raise(ConsoleRequested);
            removeButton.Click += (s, e) => Raise(RemoveRequested);

            Controls.Add(avatarBox);
            Controls.Add(nicknameLabel);
            Controls.Add(statusDot);
            Controls.Add(statusLabel);
            Controls.Add(connectButton);
            Controls.Add(settingsButton);
            Controls.Add(consoleButton);
            Controls.Add(removeButton);

            LoadAvatar();
        }

        public AfkAccount Account { get; private set; }

        public event EventHandler<AfkAccount> ToggleConnectionRequested;
        public event EventHandler<AfkAccount> SettingsRequested;
        public event EventHandler<AfkAccount> ConsoleRequested;
        public event EventHandler<AfkAccount> RemoveRequested;

        /// <summary>Kartı bir oturuma bağlar. Aynı kart yeniden bağlanabilir; eski abonelik bırakılır.</summary>
        public void Bind(AfkSession newSession)
        {
            Detach();

            session = newSession;
            if (session != null)
                session.Changed += OnSessionChanged;

            RefreshStatus();
        }

        public void Detach()
        {
            if (session == null)
                return;

            session.Changed -= OnSessionChanged;
            session = null;
        }

        /// <summary>Kullanıcı adı değiştiyse etiketi ve kafa görselini tazeler.</summary>
        public void RefreshNickname()
        {
            nicknameLabel.Text = Account.Nickname;
            LoadAvatar();
        }

        /// <summary>Sıralı bağlanma sırasında bu karta geri sayım gösterir (saniyede bir çağrılır).</summary>
        public void SetCountdown(int seconds)
        {
            pendingCountdown = seconds;
            RefreshStatus();
        }

        /// <summary>Geri sayımı kaldırır ve durumu normal görünümüne döndürür.</summary>
        public void ClearCountdown()
        {
            if (pendingCountdown == null)
                return;

            pendingCountdown = null;
            RefreshStatus();
        }

        /// <summary>Bağlı kalma süresi saniyede bir tazelendiği için dışarıdan da çağrılır.</summary>
        public void RefreshStatus()
        {
            AfkState state = session == null ? AfkState.Stopped : session.State;
            bool running = session != null && session.IsRunning;

            // Geri sayım yalnızca hesap henüz bağlanmadıysa gösterilir; bağlanınca gerçek durum öne geçer.
            if (pendingCountdown.HasValue && !running)
            {
                statusDot.FillColor = AmberTone;
                statusLabel.ForeColor = MutedTone;
                statusLabel.Text = "Sırada · " + pendingCountdown.Value + " sn sonra bağlanacak";
            }
            else
            {
                statusDot.FillColor = StateColor(state);
                statusLabel.ForeColor = state == AfkState.Error ? RedTone : MutedTone;
                statusLabel.Text = BuildStatusText(state);
            }

            // Geri çekilme beklerken süreç ölüdür (running=false) ama oturum hâlâ etkindir: buton "Kes"
            // kalıp bekleyen yeniden bağlanmayı iptal edebilmeli.
            bool active = running || state == AfkState.Reconnecting;
            connectButton.Text = active ? "Kes" : "Bağla";
            connectButton.FillColor = active ? Color.FromArgb(71, 76, 88) : GreenTone;
            connectButton.HoverState.FillColor = active ? RedTone : Color.FromArgb(22, 163, 74);

            // Başlatma/kapatma sürerken tıklamalar süreci ikiye bölmesin.
            connectButton.Enabled = session == null || !session.IsBusy || state == AfkState.Reconnecting;
            consoleButton.Enabled = session != null;
        }

        private string BuildStatusText(AfkState state)
        {
            if (session == null)
                return "Bağlı değil";

            switch (state)
            {
                case AfkState.Stopped:
                    return "Bağlı değil";

                case AfkState.Starting:
                    return "Hazırlanıyor…";

                case AfkState.Connecting:
                    return "Bağlanıyor…";

                case AfkState.Stopping:
                    return "Kapatılıyor…";

                case AfkState.Reconnecting:
                case AfkState.Error:
                    return session.StatusDetail;

                case AfkState.Connected:
                    return BuildConnectedText();

                default:
                    return session.StatusDetail;
            }
        }

        private string BuildConnectedText()
        {
            string text = "Oyunda";

            if (session.ConnectedAt.HasValue)
                text += " · " + FormatUptime(DateTime.Now - session.ConnectedAt.Value);

            if (session.Health.HasValue && session.Food.HasValue)
                text += " · " + Math.Round(session.Health.Value) + " can, " + session.Food.Value + " tokluk";

            if (session.OnlinePlayers.HasValue)
                text += " · " + session.OnlinePlayers.Value + " oyuncu";

            return text;
        }

        private static string FormatUptime(TimeSpan span)
        {
            if (span < TimeSpan.Zero)
                span = TimeSpan.Zero;

            if (span.TotalHours >= 1)
                return (int)span.TotalHours + " sa " + span.Minutes + " dk";
            if (span.TotalMinutes >= 1)
                return span.Minutes + " dk " + span.Seconds + " sn";

            return span.Seconds + " sn";
        }

        private static Color StateColor(AfkState state)
        {
            switch (state)
            {
                case AfkState.Connected: return GreenTone;
                case AfkState.Error: return RedTone;
                case AfkState.Stopped: return MutedTone;
                default: return AmberTone; // Starting / Connecting / Reconnecting / Stopping
            }
        }

        private Guna2Button MakeIconButton(string glyph, string tooltip, Color fill)
        {
            Guna2Button button = new Guna2Button
            {
                Size = new Size(36, 36),
                BorderRadius = 8,
                FillColor = fill,
                BackColor = Surface, // simge butonu köşeleri kart yüzeyine karışsın
                ForeColor = Color.White,
                Font = new Font("Segoe UI", 11f, FontStyle.Bold),
                Cursor = Cursors.Hand,
                Text = glyph,
            };

            button.HoverState.FillColor = ControlPaint.Light(fill, 0.15f);
            button.DisabledState.FillColor = Color.FromArgb(44, 48, 58);
            button.DisabledState.ForeColor = Color.FromArgb(96, 102, 116);

            iconTips.SetToolTip(button, tooltip);

            return button;
        }

        private async void LoadAvatar()
        {
            string nickname = Account.Nickname;
            Image head = await AfkAvatar.GetHeadAsync(nickname).ConfigureAwait(true);

            // Kart kapanmış ya da kullanıcı adı arada değişmiş olabilir.
            if (IsDisposed || !string.Equals(nickname, Account.Nickname, StringComparison.Ordinal))
                return;

            avatarBox.Image = head;
        }

        private void OnSessionChanged(object sender, EventArgs e)
        {
            if (IsDisposed || !IsHandleCreated)
                return;

            try
            {
                BeginInvoke((Action)RefreshStatus);
            }
            catch (ObjectDisposedException)
            {
                // Kart olay tetiklenirken kapatılmış olabilir.
            }
            catch (InvalidOperationException)
            {
                // Pencere tanıtıcısı henüz/artık yok.
            }
        }

        private void Raise(EventHandler<AfkAccount> handler)
        {
            if (handler != null)
                handler(this, Account);
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                Detach();
                iconTips.Dispose();
            }

            base.Dispose(disposing);
        }
    }
}
