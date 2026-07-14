using System;
using System.Collections.Generic;
using System.Drawing;
using System.Linq;
using System.Text.RegularExpressions;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;
using Guna.UI2.WinForms;

namespace Projects_Launcher.Afk
{
    /// <summary>
    /// "AFK Hesapları" ekranı. Ana pencerede ayarlar panelinin kardeşi olarak durur, tam ekranı kaplar.
    /// Hesap listesi, ekleme alanı ve toplu eylemler burada; tek hesabın ayrıntısı karta ve
    /// ayar penceresine devredilir.
    /// </summary>
    public class AfkPanel : Guna2Panel
    {
        private static readonly Color Background = Color.FromArgb(20, 22, 27);
        private static readonly Color Surface = Color.FromArgb(32, 36, 44);
        private static readonly Color BorderTone = Color.FromArgb(52, 58, 70);
        private static readonly Color TextTone = Color.FromArgb(245, 247, 250);
        private static readonly Color MutedTone = Color.FromArgb(152, 162, 179);
        private static readonly Color GreenTone = Color.FromArgb(34, 197, 94);
        private static readonly Color RedTone = Color.FromArgb(239, 68, 68);
        private static readonly Color AmberTone = Color.FromArgb(245, 158, 11);

        // Sıralı "Tümünü Bağla": her hesap bir öncekinden bu kadar saniye sonra bağlanır.
        private const int StaggerSeconds = 15;

        private readonly FlowLayoutPanel accountFlow;
        private readonly Guna2Panel addRow;
        private readonly Guna2TextBox newNicknameBox;
        private readonly Guna2Button addButton;
        private readonly Guna2Button connectAllButton;
        private readonly Guna2Button disconnectAllButton;
        private readonly Label emptyHintLabel;
        private readonly Label footerLabel;
        private readonly Guna2ProgressBar downloadBar;
        // System.Threading.Timer ile karışmasın: arayüz thread'inde tetiklenen WinForms zamanlayıcısı.
        private readonly System.Windows.Forms.Timer uptimeTimer;

        private readonly List<AfkAccount> accounts;
        private readonly Dictionary<string, AfkAccountCard> cards = new Dictionary<string, AfkAccountCard>();

        // Hesap başına en fazla bir konsol penceresi; pencereler modal olmadığından
        // birden fazla hesabın konsolu aynı anda açık kalabilir.
        private readonly Dictionary<string, AfkConsoleForm> consoles = new Dictionary<string, AfkConsoleForm>();

        // İstemci indirme/açma işlemi sürerken ikinci bir bağlanma denemesini engeller.
        private bool preparingClient;

        // Sıralı toplu bağlanma sürüyor mu ve iptal düğmesi. Dizi çalışırken "Tümünü Bağla" iptal görevi görür.
        private bool connectAllRunning;
        private CancellationTokenSource connectAllCts;

        // Açılıştaki otomatik bağlanma, ağ yokken kullanıcıyı hata kutusuyla karşılamamalı.
        private bool silentClientErrors;

        public AfkPanel()
        {
            accounts = AfkStore.Load();

            Size = new Size(980, 468);
            FillColor = Background;
            // BackColor açıkça verilmezse yuvarlak alt kontroller (butonlar) köşelerinde
            // SystemColors.Control (açık gri) gösterir; arkalarındaki gerçek renge sabitliyoruz.
            BackColor = Background;
            BorderRadius = 0;

            Label title = new Label
            {
                AutoSize = true,
                Location = new Point(40, 20),
                Font = new Font("Segoe UI", 16f, FontStyle.Bold),
                ForeColor = TextTone,
                BackColor = Color.Transparent,
                Text = "AFK Hesapları",
            };

            Label subtitle = new Label
            {
                AutoSize = true,
                Location = new Point(42, 52),
                Font = new Font("Segoe UI", 9f),
                ForeColor = MutedTone,
                BackColor = Color.Transparent,
                Text = "Hesaplarınızı oyuna bağlı tutun, AFK davranışını hesap başına ayarlayın.",
            };

            // Sağ kenar boşluğu içerik sütunuyla (x=40..940) hizalanır.
            connectAllButton = MakeButton("Tümünü Bağla", GreenTone, 130);
            connectAllButton.Location = new Point(682, 28);
            connectAllButton.Click += OnConnectAllClicked;

            disconnectAllButton = MakeButton("Tümünü Kes", Color.FromArgb(71, 76, 88), 120);
            disconnectAllButton.HoverState.FillColor = RedTone;
            disconnectAllButton.Location = new Point(820, 28);
            disconnectAllButton.Click += OnDisconnectAllClicked;

            Guna2Separator separator = new Guna2Separator
            {
                Location = new Point(40, 84),
                Size = new Size(900, 10),
                FillColor = BorderTone,
            };

            accountFlow = new FlowLayoutPanel
            {
                Location = new Point(40, 100),
                Size = new Size(900, 320),
                FlowDirection = FlowDirection.TopDown,
                WrapContents = false,
                AutoScroll = true,
                BackColor = Background,
                Padding = new Padding(0),
            };

            emptyHintLabel = new Label
            {
                AutoSize = true,
                Font = new Font("Segoe UI", 9.5f),
                ForeColor = MutedTone,
                BackColor = Color.Transparent,
                Margin = new Padding(4, 6, 0, 12),
                Text = "Henüz AFK hesabı yok. Aşağıdaki alandan bir kullanıcı adı girerek başlayın.",
            };

            newNicknameBox = new Guna2TextBox
            {
                Location = new Point(16, 15),
                Size = new Size(300, 36),
                BorderRadius = 8,
                FillColor = Color.FromArgb(24, 27, 33),
                BackColor = Surface, // köşeler addRow yüzeyine karışsın (beyaz kalmasın)
                BorderColor = BorderTone,
                ForeColor = TextTone,
                PlaceholderText = "Yeni hesap kullanıcı adı",
                PlaceholderForeColor = Color.FromArgb(110, 118, 132),
                Font = new Font("Segoe UI", 9.75f),
                MaxLength = 16,
            };
            AfkUi.AttachKeyDown(newNicknameBox, OnNewNicknameKeyDown);

            addButton = MakeButton("Hesap Ekle", Color.FromArgb(94, 148, 255), 130);
            addButton.Location = new Point(328, 15);
            addButton.BackColor = Surface; // addRow yüzeyinde: köşeler beyaz kalmasın
            addButton.Click += OnAddClicked;

            Label addHint = new Label
            {
                AutoSize = true,
                Location = new Point(474, 25),
                Font = new Font("Segoe UI", 8.25f),
                ForeColor = MutedTone,
                BackColor = Color.Transparent,
                Text = "3-16 karakter; yalnızca harf, rakam ve alt çizgi.",
            };

            // Hesap eklendikçe akış düzeni bu satırı kendiliğinden aşağı iter.
            addRow = new Guna2Panel
            {
                Size = new Size(AfkAccountCard.CardWidth, 66),
                Margin = new Padding(0, 0, 0, 10),
                BorderRadius = 10,
                FillColor = Surface,
                BackColor = Background, // yuvarlak köşeler liste zeminine karışsın (beyaz kalmasın)
                BorderColor = BorderTone,
                BorderThickness = 1,
            };
            addRow.Controls.Add(newNicknameBox);
            addRow.Controls.Add(addButton);
            addRow.Controls.Add(addHint);

            downloadBar = new Guna2ProgressBar
            {
                Location = new Point(40, 432),
                Size = new Size(300, 6),
                FillColor = Color.FromArgb(40, 44, 52),
                BackColor = Background, // yuvarlak uçlar panel zeminine karışsın (beyaz kalmasın)
                ProgressColor = GreenTone,
                ProgressColor2 = Color.FromArgb(16, 185, 129),
                AutoRoundedCorners = true,
                ShowText = false,
                Visible = false,
            };

            footerLabel = new Label
            {
                AutoSize = true,
                Location = new Point(40, 442),
                Font = new Font("Segoe UI", 8.25f),
                ForeColor = MutedTone,
                BackColor = Color.Transparent,
                Text = "AFK istemcisi ilk bağlanışta otomatik indirilir.",
            };

            Controls.Add(title);
            Controls.Add(subtitle);
            Controls.Add(connectAllButton);
            Controls.Add(disconnectAllButton);
            Controls.Add(separator);
            Controls.Add(accountFlow);
            Controls.Add(downloadBar);
            Controls.Add(footerLabel);

            // Bağlı kalma süresi yalnızca zamanla değiştiği için oturum olayı üretmez; saniyede bir tazelenir.
            uptimeTimer = new System.Windows.Forms.Timer { Interval = 1000 };
            uptimeTimer.Tick += OnUptimeTick;

            BuildCards();
        }

        /// <summary>Panel görünür olduğunda ana pencere çağırır; görünmezken zamanlayıcı boşuna dönmesin.</summary>
        public void OnShown()
        {
            uptimeTimer.Start();
            RefreshAllCards();
        }

        public void OnHidden()
        {
            uptimeTimer.Stop();
        }

        // --- Liste kurulumu ---

        private void BuildCards()
        {
            accountFlow.SuspendLayout();
            accountFlow.Controls.Clear();

            foreach (AfkAccountCard card in cards.Values)
            {
                card.Detach();
                card.Dispose();
            }
            cards.Clear();

            if (accounts.Count == 0)
                accountFlow.Controls.Add(emptyHintLabel);

            foreach (AfkAccount account in accounts)
            {
                AfkAccountCard card = new AfkAccountCard(account);
                card.ToggleConnectionRequested += OnToggleConnection;
                card.SettingsRequested += OnSettingsRequested;
                card.ConsoleRequested += OnConsoleRequested;
                card.RemoveRequested += OnRemoveRequested;

                AfkSession session;
                card.Bind(AfkManager.Instance.TryGetSession(account.Id, out session) ? session : null);

                cards[account.Id] = card;
                accountFlow.Controls.Add(card);
            }

            accountFlow.Controls.Add(addRow);
            accountFlow.ResumeLayout();

            UpdateBulkButtons();
        }

        private void RefreshAllCards()
        {
            foreach (AfkAccountCard card in cards.Values)
                card.RefreshStatus();

            UpdateBulkButtons();
        }

        private void UpdateBulkButtons()
        {
            bool anyStopped = accounts.Any(a =>
            {
                AfkSession session;
                return !AfkManager.Instance.TryGetSession(a.Id, out session) || !IsSessionActive(session);
            });

            bool anyRunning = accounts.Any(a =>
            {
                AfkSession session;
                return AfkManager.Instance.TryGetSession(a.Id, out session) && IsSessionActive(session);
            });

            // Dizi sürerken düğme "İptal Et" olarak açık kalır; aksi halde bağlanacak hesap varsa etkin.
            connectAllButton.Enabled = connectAllRunning ||
                                       (accounts.Count > 0 && anyStopped && !preparingClient);
            disconnectAllButton.Enabled = anyRunning;
        }

        private void OnUptimeTick(object sender, EventArgs e)
        {
            RefreshAllCards();
        }

        // --- Hesap ekleme / kaldırma ---

        private void OnNewNicknameKeyDown(object sender, KeyEventArgs e)
        {
            if (e.KeyCode != Keys.Enter)
                return;

            e.SuppressKeyPress = true;
            OnAddClicked(sender, EventArgs.Empty);
        }

        private void OnAddClicked(object sender, EventArgs e)
        {
            string nickname = newNicknameBox.Text.Trim();

            if (!Regex.IsMatch(nickname, AfkDefaults.NicknamePattern))
            {
                MessageBox.Show(
                    "Kullanıcı adı 3-16 karakter olmalı ve yalnızca harf, rakam ile alt çizgi içermelidir.",
                    "Kullanıcı Adı Geçersiz", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }

            if (accounts.Any(a => string.Equals(a.Nickname, nickname, StringComparison.OrdinalIgnoreCase)))
            {
                MessageBox.Show("Bu kullanıcı adı zaten AFK hesapları arasında.", "Hesap Mevcut",
                    MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }

            AfkAccount account = new AfkAccount { Nickname = nickname };
            accounts.Add(account);
            AfkStore.Save(accounts);

            newNicknameBox.Text = string.Empty;
            BuildCards();

            // Yeni kart eklendiğinde ekleme alanı görüş alanının dışına çıkmış olabilir.
            accountFlow.ScrollControlIntoView(addRow);
            newNicknameBox.Focus();
        }

        private void OnRemoveRequested(object sender, AfkAccount account)
        {
            bool confirmed = AfkConfirm.Ask(FindForm(), "Hesabı Kaldır",
                "\"" + account.Nickname + "\" hesabı ve bu hesaba ait AFK ayarları kalıcı olarak silinecek.\n\n" +
                "Devam etmek istiyor musunuz?",
                "Kaldır", true);

            if (!confirmed)
                return;

            // Hesap silinirken açık konsolu da kapatılır; pencere ölü bir oturumu izlemesin.
            CloseConsole(account.Id);

            AfkManager.Instance.Remove(account);
            accounts.RemoveAll(a => a.Id == account.Id);
            AfkStore.Save(accounts);

            TryDeleteAccountFolder(account);
            BuildCards();
        }

        private static void TryDeleteAccountFolder(AfkAccount account)
        {
            try
            {
                string dir = AfkPaths.AccountDir(account);
                if (System.IO.Directory.Exists(dir))
                    System.IO.Directory.Delete(dir, true);
            }
            catch (Exception)
            {
                // Süreç dosyayı hâlâ tutuyor olabilir; hesap listeden düştüğü için kullanıcıyı ilgilendirmez.
            }
        }

        // --- Hesap eylemleri ---

        private void OnSettingsRequested(object sender, AfkAccount account)
        {
            using (AfkAccountSettingsForm dialog = new AfkAccountSettingsForm(account.Clone()))
            {
                if (dialog.ShowDialog(FindForm()) != DialogResult.OK)
                    return;

                int index = accounts.FindIndex(a => a.Id == account.Id);
                if (index < 0)
                    return;

                // Kimlik ve kullanıcı adı ayar penceresinde değiştirilmez; kalanı yeni ayarlarla değişir.
                AfkAccount updated = dialog.Account;
                updated.Id = account.Id;
                updated.Nickname = account.Nickname;
                accounts[index] = updated;
                AfkStore.Save(accounts);

                AfkSession session;
                if (AfkManager.Instance.TryGetSession(account.Id, out session) && session.IsRunning)
                {
                    MessageBox.Show(
                        "Ayarlar kaydedildi. Yeni ayarların geçerli olması için hesabın bağlantısını kesip yeniden bağlanın.",
                        "AFK Ayarları", MessageBoxButtons.OK, MessageBoxIcon.Information);
                }

                BuildCards();
            }
        }

        private void OnConsoleRequested(object sender, AfkAccount account)
        {
            AfkSession session;
            if (!AfkManager.Instance.TryGetSession(account.Id, out session))
                return;

            // Aynı hesabın konsolu zaten açıksa yenisini açmak yerine öne getirilir.
            AfkConsoleForm existing;
            if (consoles.TryGetValue(account.Id, out existing) && !existing.IsDisposed)
            {
                if (existing.WindowState == FormWindowState.Minimized)
                    existing.WindowState = FormWindowState.Normal;

                existing.BringToFront();
                existing.Activate();
                return;
            }

            AfkConsoleForm console = new AfkConsoleForm(session);
            consoles[account.Id] = console;

            string accountId = account.Id;
            console.FormClosed += delegate { consoles.Remove(accountId); };

            PositionConsole(console);

            // Modal değil: birden fazla hesabın konsolu aynı anda açık kalabilir ve
            // başlatıcı bu sırada kullanılmaya devam edebilir.
            console.Show();
        }

        /// <summary>Konsolları basamaklı dizer; pencere her durumda ekranın içinde kalır.</summary>
        private void PositionConsole(AfkConsoleForm console)
        {
            Form owner = FindForm();

            int step = 32 * ((consoles.Count - 1) % 8);
            Point basePoint = owner != null
                ? new Point(owner.Left + 70 + step, owner.Top + 60 + step)
                : new Point(120 + step, 120 + step);

            Rectangle area = Screen.FromPoint(basePoint).WorkingArea;
            int x = Math.Max(area.Left, Math.Min(basePoint.X, area.Right - console.Width));
            int y = Math.Max(area.Top, Math.Min(basePoint.Y, area.Bottom - console.Height));

            console.Location = new Point(x, y);
        }

        private void CloseConsole(string accountId)
        {
            AfkConsoleForm console;
            if (!consoles.TryGetValue(accountId, out console))
                return;

            consoles.Remove(accountId);
            if (!console.IsDisposed)
                console.Close();
        }

        private async void OnToggleConnection(object sender, AfkAccount account)
        {
            AfkSession existing;
            if (AfkManager.Instance.TryGetSession(account.Id, out existing) && IsSessionActive(existing))
            {
                existing.Stop();
                RefreshAllCards();
                return;
            }

            await ConnectAsync(account).ConfigureAwait(true);
        }

        // Hesaplar aynı anda değil, sırayla bağlanır: ilki hemen, sonrakiler bir öncekinden
        // StaggerSeconds saniye sonra. Bekleyen hesabın kartında geri sayım gösterilir. Dizi
        // sürerken bu düğme "İptal Et"e döner; tekrar tıklanırsa kalan bağlanmalar durdurulur.
        private async void OnConnectAllClicked(object sender, EventArgs e)
        {
            if (connectAllRunning)
            {
                if (connectAllCts != null)
                    connectAllCts.Cancel();
                return;
            }

            List<AfkAccount> pending = accounts.Where(a => !IsRunning(a)).ToList();
            if (pending.Count == 0)
                return;

            connectAllRunning = true;
            connectAllCts = new CancellationTokenSource();
            CancellationToken token = connectAllCts.Token;

            connectAllButton.Text = "İptal Et";
            connectAllButton.FillColor = AmberTone;
            UpdateBulkButtons();

            try
            {
                bool firstConnected = false;
                foreach (AfkAccount account in pending)
                {
                    if (token.IsCancellationRequested)
                        break;

                    // Autorelog vb. yüzünden bu arada bağlanmış olabilir; onu atla.
                    if (IsRunning(account))
                        continue;

                    // İlk hesap hemen bağlanır; sonrakiler kendi kartında geri sayımı bekler.
                    if (firstConnected)
                    {
                        bool completed = await CountdownAsync(account, StaggerSeconds, token).ConfigureAwait(true);
                        if (!completed || token.IsCancellationRequested)
                            break;

                        // Kullanıcı beklerken bu hesabı elle bağlamış olabilir.
                        if (IsRunning(account))
                            continue;
                    }

                    await ConnectAsync(account).ConfigureAwait(true);
                    firstConnected = true;
                }
            }
            finally
            {
                connectAllRunning = false;
                if (connectAllCts != null)
                {
                    connectAllCts.Dispose();
                    connectAllCts = null;
                }

                // Panel dizi sürerken kapanmış olabilir (uygulama çıkışı); ölü kontrole dokunma.
                if (!IsDisposed)
                {
                    ClearAllCountdowns();
                    connectAllButton.Text = "Tümünü Bağla";
                    connectAllButton.FillColor = GreenTone;
                    UpdateBulkButtons();
                }
            }
        }

        private static bool IsRunning(AfkAccount account)
        {
            AfkSession session;
            return AfkManager.Instance.TryGetSession(account.Id, out session) && session.IsRunning;
        }

        // Süreç çalışıyor ya da geri çekilme beklerken yeniden bağlanmayı sürdürüyor. İkinci durumda
        // süreç ölüdür (IsRunning=false) ama kullanıcı için oturum hâlâ etkindir: "Tümünü Bağla" onu
        // atlamalı, "Tümünü Kes" ve kart düğmesi ise bekleyen yeniden başlatmayı durdurabilmeli.
        private static bool IsSessionActive(AfkSession session)
        {
            return session != null && (session.IsRunning || session.State == AfkState.Reconnecting);
        }

        /// <summary>
        /// Bir hesabın kartında saniye saniye geri sayım gösterir. Süre dolarsa true; iptal edilirse
        /// (ya da hesap bu arada bağlanırsa) döngünün buna göre davranması için erken döner.
        /// </summary>
        private async Task<bool> CountdownAsync(AfkAccount account, int seconds, CancellationToken token)
        {
            try
            {
                for (int remaining = seconds; remaining > 0; remaining--)
                {
                    if (token.IsCancellationRequested)
                        return false;
                    if (IsRunning(account))
                        return true;

                    SetCardCountdown(account, remaining);

                    try
                    {
                        await Task.Delay(1000, token).ConfigureAwait(true);
                    }
                    catch (TaskCanceledException)
                    {
                        return false;
                    }
                }

                return true;
            }
            finally
            {
                ClearCardCountdown(account);
            }
        }

        private void SetCardCountdown(AfkAccount account, int seconds)
        {
            if (IsDisposed)
                return;

            AfkAccountCard card;
            if (cards.TryGetValue(account.Id, out card))
                card.SetCountdown(seconds);
        }

        private void ClearCardCountdown(AfkAccount account)
        {
            if (IsDisposed)
                return;

            AfkAccountCard card;
            if (cards.TryGetValue(account.Id, out card))
                card.ClearCountdown();
        }

        private void ClearAllCountdowns()
        {
            foreach (AfkAccountCard card in cards.Values)
                card.ClearCountdown();
        }

        private void OnDisconnectAllClicked(object sender, EventArgs e)
        {
            // Sıralı bağlanma sürüyorsa önce onu durdur: bekleyen hesaplar bağlanmaya devam etmesin.
            if (connectAllCts != null)
                connectAllCts.Cancel();

            foreach (AfkSession session in AfkManager.Instance.Sessions.ToList())
            {
                if (IsSessionActive(session))
                    session.Stop();
            }

            RefreshAllCards();
        }

        // --- İstemciyi hazırla ve bağlan ---

        private async Task ConnectAsync(AfkAccount account)
        {
            string exePath = await PrepareClientAsync().ConfigureAwait(true);
            if (exePath == null)
                return;

            AfkSession session = AfkManager.Instance.GetSession(account);

            // Yeni oluşturulan oturumun kartı henüz ona bağlı değil.
            AfkAccountCard card;
            if (cards.TryGetValue(account.Id, out card))
                card.Bind(session);

            session.Start(exePath, Properties.Settings.Default.NickNames);
            RefreshAllCards();
        }

        /// <summary>İstemciyi gerekirse indirir/günceller. Başarısızlıkta kullanıcıya haber verir ve null döndürür.</summary>
        private async Task<string> PrepareClientAsync()
        {
            string existing = AfkRuntime.ResolveExistingExe();

            // Çalışan bir oturum varken istemci exe'si kilitlidir; güncelleme denemesi
            // paketi açarken hata verirdi. Kurulu sürümle devam edip güncellemeyi sonraya bırakıyoruz.
            if (existing != null && AfkManager.Instance.Sessions.Any(s => s.IsRunning))
                return existing;

            if (preparingClient)
                return null;

            preparingClient = true;
            UpdateBulkButtons();

            downloadBar.Value = 0;
            downloadBar.Visible = true;

            Progress<AfkDownloadProgress> progress = new Progress<AfkDownloadProgress>(p =>
            {
                footerLabel.Text = p.TotalBytes > 0
                    ? p.Phase + " %" + p.Percent
                    : p.Phase;
                downloadBar.Value = p.Percent;
            });

            try
            {
                return await AfkRuntime.EnsureClientAsync(progress, CancellationToken.None).ConfigureAwait(true);
            }
            catch (Exception ex)
            {
                if (!silentClientErrors)
                {
                    MessageBox.Show(
                        "AFK istemcisi indirilemedi.\n\nİnternet bağlantınızı denetleyip tekrar deneyin.\n\n" + ex.Message,
                        "AFK İstemcisi", MessageBoxButtons.OK, MessageBoxIcon.Error);
                }

                return null;
            }
            finally
            {
                preparingClient = false;
                downloadBar.Visible = false;
                footerLabel.Text = "AFK istemcisi ilk bağlanışta otomatik indirilir.";
                UpdateBulkButtons();
            }
        }

        // --- Otomatik bağlanma ---

        /// <summary>Başlatıcı açılırken "otomatik bağlan" işaretli hesapları ayağa kaldırır.</summary>
        public async Task StartAutoConnectAccountsAsync()
        {
            List<AfkAccount> autoStart = accounts.Where(a => a.AutoStart).ToList();
            if (autoStart.Count == 0)
                return;

            silentClientErrors = true;
            try
            {
                foreach (AfkAccount account in autoStart)
                    await ConnectAsync(account).ConfigureAwait(true);
            }
            finally
            {
                silentClientErrors = false;
            }
        }

        private Guna2Button MakeButton(string text, Color fill, int width)
        {
            Guna2Button button = new Guna2Button
            {
                Size = new Size(width, 34),
                BorderRadius = 8,
                FillColor = fill,
                // Varsayılan: panel yüzeyi. addRow üzerindeki buton çağrı yerinde Surface'a çevrilir.
                BackColor = Background,
                ForeColor = Color.White,
                Font = new Font("Segoe UI", 9f, FontStyle.Bold),
                Cursor = Cursors.Hand,
                Text = text,
            };

            button.HoverState.FillColor = ControlPaint.Light(fill, 0.15f);
            button.DisabledState.FillColor = Color.FromArgb(44, 48, 58);
            button.DisabledState.ForeColor = Color.FromArgb(96, 102, 116);
            return button;
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                if (connectAllCts != null)
                    connectAllCts.Cancel();

                uptimeTimer.Stop();
                uptimeTimer.Dispose();
            }

            base.Dispose(disposing);
        }
    }
}
