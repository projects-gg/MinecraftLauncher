using Guna.UI2.WinForms;
using System;
using System.Diagnostics;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Threading;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace Projects_Launcher.Auth
{
    /// <summary>
    /// Premium (satın alınmış) Minecraft hesabıyla giriş penceresi.
    /// <para>
    /// Kullanıcı parolasını yalnızca Microsoft'un kendi sayfasına girer; bu pencere akışı başlatır
    /// ve hangi adımda olunduğunu gösterir. Giriş başarılıysa <see cref="DialogResult.OK"/> döner ve
    /// oturum <see cref="PremiumSession"/> içine kaydedilmiş olur.
    /// </para>
    /// </summary>
    public sealed class PremiumLoginForm : Form
    {
        // Başlatıcının geri kalanıyla aynı palet.
        private static readonly Color CardColor = Color.FromArgb(32, 36, 44);
        private static readonly Color BorderColor = Color.FromArgb(52, 58, 70);
        private static readonly Color TextPrimary = Color.FromArgb(245, 247, 250);
        private static readonly Color TextMuted = Color.FromArgb(152, 162, 179);
        private static readonly Color TextFaint = Color.FromArgb(108, 117, 132);
        private static readonly Color WarningColor = Color.FromArgb(245, 158, 11);
        private static readonly Color WarningSurface = Color.FromArgb(45, 38, 22);
        private static readonly Color WarningText = Color.FromArgb(198, 178, 142);
        private static readonly Color SuccessColor = Color.FromArgb(34, 197, 94);
        private static readonly Color DangerColor = Color.FromArgb(239, 68, 68);
        private static readonly Color BrandColor = Color.FromArgb(248, 148, 35);

        private const int CardWidth = 470;
        private const int Gutter = 28;
        private const int StepRowHeight = 33;

        // Kullanıcı tarayıcıda takılıp kalırsa pencere sonsuza kadar beklemesin.
        private static readonly TimeSpan SignInTimeout = TimeSpan.FromMinutes(5);

        // Giriş başlamadan önce durum alanında duran açıklama (alan hiç boş görünmesin).
        private const string IdleMessage =
            "Giriş sayfası varsayılan tarayıcınızda açılır.\nOnayladıktan sonra bu pencere kendiliğinden devam eder.";

        // Giriş başarısız olduğunda açılan destek talebi formu.
        private const string SupportUrl = "https://mc.projects.gg/support/tickets/submit/";

        // Alt satır: normalde gizlilik notu, hata durumunda hata kodu ve paylaşım isteği.
        private const string PrivacyNote = "Parolanız yalnızca Microsoft'un giriş sayfasına yazılır.";

        private static readonly string[] StepTitles =
        {
            "Tarayıcıda Microsoft hesabınıza giriş yapın",
            "Xbox Live doğrulanıyor",
            "Minecraft oturumu açılıyor",
            "Lisans ve profil denetleniyor",
        };

        private readonly Panel _content;
        private readonly StepIndicator[] _indicators = new StepIndicator[StepTitles.Length];
        private readonly Label[] _stepLabels = new Label[StepTitles.Length];
        private readonly Panel _stepsPanel;
        private readonly Label _successLabel;
        private readonly Label _messageLabel;
        private readonly Guna2Button _signInButton;
        private readonly Guna2Button _cancelButton;
        private readonly Guna2Button _copyButton;
        private readonly Guna2Button _supportButton;
        private readonly Label _footerLabel;
        private readonly System.Windows.Forms.Timer _spinnerTimer;

        // Adım listesi giriş başlayana kadar gizlidir; "Giriş Yap"a basılınca pencere yumuşakça
        // uzar ve adımlar sırayla belirir. Boş halkalar durağan haldeyken tıklanabilir düğme gibi
        // görünüp ekranın ne anlattığını bulanıklaştırıyordu.
        private readonly System.Windows.Forms.Timer _revealTimer;
        private readonly Control[] _controlsBelowSteps;
        private readonly int[] _expandedTops;
        private readonly int _collapsedHeight;
        private readonly int _expandedHeight;
        private bool _expanded;
        private int _revealedSteps;

        // "Kopyalandı" geri bildirimini kısa süre sonra eski haline döndürür.
        private readonly System.Windows.Forms.Timer _copyFeedbackTimer;

        private CancellationTokenSource _cancellation;
        private bool _cancelledByUser;
        private int _activeStep = -1;
        private string _errorCode;
        private string _errorDiagnostics;

        /// <summary>Giriş başarılıysa açılan hesap; aksi halde <c>null</c>.</summary>
        public PremiumAccount Account { get; private set; }

        /// <summary>Pencereyi açar ve giriş başarılıysa hesabı döndürür; iptal/hata durumunda <c>null</c>.</summary>
        public static PremiumAccount Prompt(IWin32Window owner)
        {
            // Özellik kapalıyken pencere hiç açılmaz (butonu da zaten gösterilmez).
            if (!PremiumSession.IsEnabled)
                return null;

            using (PremiumLoginForm form = new PremiumLoginForm())
            {
                return form.ShowDialog(owner) == DialogResult.OK ? form.Account : null;
            }
        }

        public PremiumLoginForm()
        {
            SuspendLayout();

            FormBorderStyle = FormBorderStyle.None;
            StartPosition = FormStartPosition.CenterParent;
            ShowInTaskbar = false;
            KeyPreview = true;
            DoubleBuffered = true;
            // Form yüzeyi kenarlık rengiyle boyanır; 1px içerideki panel kartı oluşturur.
            BackColor = BorderColor;
            ClientSize = new Size(CardWidth + 2, 100); // yükseklik içerik yerleştikten sonra kesinleşir

            Guna2Elipse formElipse = new Guna2Elipse();
            formElipse.TargetControl = this;
            formElipse.BorderRadius = 14;

            _content = new Panel();
            _content.Location = new Point(1, 1);
            _content.Size = new Size(CardWidth, 100);
            _content.BackColor = CardColor;
            Controls.Add(_content);

            int y = BuildHeader();
            int warningBottom = BuildWarningCard(y + 16);
            y = warningBottom;

            _stepsPanel = new Panel();
            _stepsPanel.Location = new Point(Gutter, y + 22);
            _stepsPanel.Size = new Size(ContentWidth, StepTitles.Length * StepRowHeight);
            _stepsPanel.BackColor = CardColor;
            _content.Controls.Add(_stepsPanel);

            BuildSteps();

            // Başarı görünümü adım listesinin yerini alır (aynı konum, aynı boyut).
            _successLabel = new Label();
            _successLabel.AutoSize = false;
            _successLabel.Location = _stepsPanel.Location;
            _successLabel.Size = _stepsPanel.Size;
            _successLabel.TextAlign = ContentAlignment.MiddleCenter;
            _successLabel.Font = new Font("Segoe UI", 14f, FontStyle.Bold);
            _successLabel.ForeColor = SuccessColor;
            _successLabel.BackColor = CardColor;
            _successLabel.Visible = false;
            _content.Controls.Add(_successLabel);

            _messageLabel = new Label();
            _messageLabel.AutoSize = false;
            _messageLabel.Location = new Point(Gutter, _stepsPanel.Bottom + 14);
            _messageLabel.Size = new Size(ContentWidth, 52);
            _messageLabel.Font = new Font("Segoe UI", 9f, FontStyle.Regular);
            _messageLabel.ForeColor = TextMuted;
            _messageLabel.Text = IdleMessage;
            _content.Controls.Add(_messageLabel);

            _signInButton = new Guna2Button();
            _signInButton.Text = "Microsoft ile Giriş Yap";
            _signInButton.Font = new Font("Segoe UI Semibold", 10.5f, FontStyle.Bold);
            _signInButton.ForeColor = Color.FromArgb(26, 29, 35);
            _signInButton.FillColor = Color.White;
            _signInButton.BackColor = CardColor;
            _signInButton.HoverState.FillColor = Color.FromArgb(232, 236, 242);
            _signInButton.PressedColor = Color.FromArgb(214, 220, 228);
            _signInButton.DisabledState.FillColor = Color.FromArgb(62, 68, 80);
            _signInButton.DisabledState.ForeColor = TextFaint;
            _signInButton.BorderRadius = 10;
            _signInButton.Size = new Size(ContentWidth, 46);
            _signInButton.Location = new Point(Gutter, _messageLabel.Bottom + 12);
            _signInButton.Cursor = Cursors.Hand;
            _signInButton.Image = MicrosoftBrand.CreateLogo(20);
            _signInButton.ImageSize = new Size(20, 20);
            _signInButton.ImageAlign = HorizontalAlignment.Left;
            _signInButton.ImageOffset = new Point(16, 0);
            _signInButton.TextOffset = new Point(14, 0);
            _signInButton.Click += OnSignInClick;
            _content.Controls.Add(_signInButton);

            _cancelButton = new Guna2Button();
            _cancelButton.Text = "Vazgeç";
            _cancelButton.Font = new Font("Segoe UI", 9.5f, FontStyle.Regular);
            _cancelButton.ForeColor = TextMuted;
            _cancelButton.FillColor = CardColor;
            _cancelButton.BackColor = CardColor;
            _cancelButton.HoverState.FillColor = Color.FromArgb(44, 49, 60);
            _cancelButton.BorderRadius = 8;
            _cancelButton.Size = new Size(ContentWidth, 32);
            _cancelButton.Location = new Point(Gutter, _signInButton.Bottom + 8);
            _cancelButton.Cursor = Cursors.Hand;
            _cancelButton.Click += OnCancelClick;
            _content.Controls.Add(_cancelButton);

            // Hata ayrıntısını panoya alır; destek talebine yapıştırılabilsin diye.
            _copyButton = new Guna2Button();
            _copyButton.Text = "Kodu Kopyala";
            _copyButton.Font = new Font("Segoe UI", 9.5f, FontStyle.Regular);
            _copyButton.ForeColor = TextMuted;
            _copyButton.FillColor = CardColor;
            _copyButton.BackColor = CardColor;
            _copyButton.BorderColor = Color.FromArgb(74, 81, 95);
            _copyButton.BorderThickness = 1;
            _copyButton.HoverState.FillColor = Color.FromArgb(44, 49, 60);
            _copyButton.BorderRadius = 8;
            _copyButton.Size = new Size(ActionWidth, 32);
            _copyButton.Location = new Point(Gutter + ActionWidth + ActionGap, _cancelButton.Top);
            _copyButton.Cursor = Cursors.Hand;
            _copyButton.Visible = false;
            _copyButton.Click += OnCopyCodeClick;
            _content.Controls.Add(_copyButton);

            // Yalnızca giriş başarısız olduğunda görünür; alt satırı üçe böler.
            _supportButton = new Guna2Button();
            _supportButton.Text = "Destek Talebi Aç";
            _supportButton.Font = new Font("Segoe UI Semibold", 9.5f, FontStyle.Bold);
            _supportButton.ForeColor = BrandColor;
            _supportButton.FillColor = CardColor;
            _supportButton.BackColor = CardColor;
            _supportButton.BorderColor = BrandColor;
            _supportButton.BorderThickness = 1;
            _supportButton.HoverState.FillColor = BrandColor;
            _supportButton.HoverState.ForeColor = Color.FromArgb(26, 29, 35);
            _supportButton.BorderRadius = 8;
            _supportButton.Size = new Size(ContentWidth - ((ActionWidth + ActionGap) * 2), 32);
            _supportButton.Location = new Point(Gutter + ((ActionWidth + ActionGap) * 2), _cancelButton.Top);
            _supportButton.Cursor = Cursors.Hand;
            _supportButton.Visible = false;
            _supportButton.Click += OnSupportClick;
            _content.Controls.Add(_supportButton);

            _footerLabel = new Label();
            _footerLabel.AutoSize = false;
            _footerLabel.Location = new Point(Gutter, _cancelButton.Bottom + 8);
            _footerLabel.Size = new Size(ContentWidth, 18);
            _footerLabel.TextAlign = ContentAlignment.MiddleCenter;
            _footerLabel.Font = new Font("Segoe UI", 8f, FontStyle.Regular);
            _footerLabel.ForeColor = TextFaint;
            _footerLabel.Text = PrivacyNote;
            _content.Controls.Add(_footerLabel);

            // Yükseklik içeriğe göre kesinleşir: yerleşim değişirse pencere kendiliğinden uyum sağlar.
            _expandedHeight = _footerLabel.Bottom + 18;

            // Adım bloğu gizliyken altındaki her şey yukarı çekilir ve pencere o kadar kısalır.
            _controlsBelowSteps = new Control[]
            {
                _messageLabel, _signInButton, _cancelButton, _copyButton, _supportButton, _footerLabel,
            };
            _expandedTops = new int[_controlsBelowSteps.Length];
            for (int i = 0; i < _controlsBelowSteps.Length; i++)
                _expandedTops[i] = _controlsBelowSteps[i].Top;

            _collapsedHeight = _expandedHeight - (_messageLabel.Top - (warningBottom + 18));

            _stepsPanel.Visible = false;
            for (int i = 0; i < _indicators.Length; i++)
            {
                _indicators[i].Visible = false;
                _stepLabels[i].Visible = false;
            }

            ApplyCardHeight(_collapsedHeight);

            // Etkin adımın dönen halkası; yalnızca giriş sürerken çalışır.
            _spinnerTimer = new System.Windows.Forms.Timer();
            _spinnerTimer.Interval = 60;
            _spinnerTimer.Tick += OnSpinnerTick;

            // Pencerenin açılması ve adımların tek tek belirmesi aynı zamanlayıcıyla yürür.
            _revealTimer = new System.Windows.Forms.Timer();
            _revealTimer.Interval = 15;
            _revealTimer.Tick += OnRevealTick;

            _copyFeedbackTimer = new System.Windows.Forms.Timer();
            _copyFeedbackTimer.Interval = 1600;
            _copyFeedbackTimer.Tick += OnCopyFeedbackTick;

            KeyDown += OnFormKeyDown;

            ResumeLayout(false);
        }

        private int ContentWidth
        {
            get { return CardWidth - (Gutter * 2); }
        }

        // Hata durumunda alt satır üçe bölünür: Vazgeç · Kodu Kopyala · Destek Talebi Aç
        private const int ActionGap = 8;

        private int ActionWidth
        {
            get { return (ContentWidth - (ActionGap * 2)) / 3; }
        }

        // --- Açılma animasyonu ---

        /// <summary>
        /// Kart yüksekliğini uygular ve adım bloğunun altındaki denetimleri buna göre kaydırır.
        /// Pencerenin dikey merkezi korunur: açılırken tek yöne doğru kaymış gibi görünmesin.
        /// </summary>
        private void ApplyCardHeight(int cardHeight)
        {
            int shift = _expandedHeight - cardHeight;

            for (int i = 0; i < _controlsBelowSteps.Length; i++)
                _controlsBelowSteps[i].Top = _expandedTops[i] - shift;

            // Kurucu sırasında pencere henüz konumlanmadığı için merkezleme yalnızca ekrandayken yapılır.
            bool onScreen = IsHandleCreated;
            int centerY = onScreen ? Top + (Height / 2) : 0;

            _content.Height = cardHeight;
            ClientSize = new Size(CardWidth + 2, cardHeight + 2);

            if (onScreen)
                Top = centerY - (Height / 2);
        }

        // Giriş başlarken bir kez çalışır: pencere adım listesine yer açacak kadar uzar.
        private void BeginExpand()
        {
            if (_expanded)
                return;

            _expanded = true;
            _revealedSteps = 0;
            _stepsPanel.Visible = true;
            _revealTimer.Interval = 15;
            _revealTimer.Start();
        }

        private void OnRevealTick(object sender, EventArgs e)
        {
            if (_content.Height < _expandedHeight)
            {
                // Sona doğru yavaşlayan büyüme; sabit adım mekanik görünüyordu.
                int remaining = _expandedHeight - _content.Height;
                ApplyCardHeight(Math.Min(_expandedHeight, _content.Height + Math.Max(6, remaining / 3)));
                return;
            }

            if (_revealedSteps < _indicators.Length)
            {
                _indicators[_revealedSteps].Visible = true;
                _stepLabels[_revealedSteps].Visible = true;
                _revealedSteps++;
                _revealTimer.Interval = 70; // adımlar tek tek belirsin
                return;
            }

            _revealTimer.Stop();
        }

        // --- Kurulum ---

        private int BuildHeader()
        {
            Label closeButton = new Label();
            closeButton.AutoSize = false;
            closeButton.Size = new Size(32, 28);
            closeButton.Location = new Point(CardWidth - 32 - 12, 12);
            closeButton.TextAlign = ContentAlignment.MiddleCenter;
            closeButton.Font = new Font("Segoe UI", 11f, FontStyle.Regular);
            closeButton.ForeColor = TextFaint;
            closeButton.Text = "✕";
            closeButton.Cursor = Cursors.Hand;
            closeButton.MouseEnter += delegate { closeButton.ForeColor = TextPrimary; };
            closeButton.MouseLeave += delegate { closeButton.ForeColor = TextFaint; };
            closeButton.Click += OnCancelClick;
            _content.Controls.Add(closeButton);

            Label badge = new Label();
            badge.AutoSize = false;
            badge.Location = new Point(Gutter, 24);
            badge.Size = new Size(92, 22);
            badge.TextAlign = ContentAlignment.MiddleCenter;
            badge.Font = new Font("Segoe UI Semibold", 8.25f, FontStyle.Bold);
            badge.ForeColor = BrandColor;
            badge.BackColor = Color.FromArgb(46, 37, 24);
            badge.Text = "PREMİUM";
            _content.Controls.Add(badge);

            Guna2Elipse badgeElipse = new Guna2Elipse();
            badgeElipse.TargetControl = badge;
            badgeElipse.BorderRadius = 11;

            Label title = new Label();
            title.AutoSize = false;
            title.Location = new Point(Gutter, 54);
            title.Size = new Size(ContentWidth, 32);
            title.Font = new Font("Segoe UI", 16f, FontStyle.Bold);
            title.ForeColor = TextPrimary;
            title.Text = "Orijinal Hesapla Giriş";
            _content.Controls.Add(title);

            // Pencerenin başlık çubuğu yok; başlık alanından sürüklenir.
            Guna2DragControl dragControl = new Guna2DragControl();
            dragControl.TargetControl = title;

            Label subtitle = new Label();
            subtitle.AutoSize = false;
            subtitle.Location = new Point(Gutter, 88);
            subtitle.Size = new Size(ContentWidth, 20);
            subtitle.Font = new Font("Segoe UI", 9.5f, FontStyle.Regular);
            subtitle.ForeColor = TextMuted;
            subtitle.Text = "Microsoft hesabınızla girin, oyuna çevrimiçi modda bağlanın.";
            _content.Controls.Add(subtitle);

            return subtitle.Bottom;
        }

        private int BuildWarningCard(int top)
        {
            Panel card = new Panel();
            card.Location = new Point(Gutter, top);
            card.Size = new Size(ContentWidth, 84);
            card.BackColor = WarningSurface;
            _content.Controls.Add(card);

            Guna2Elipse cardElipse = new Guna2Elipse();
            cardElipse.TargetControl = card;
            cardElipse.BorderRadius = 10;

            // Sol kenardaki ince şerit uyarıyı bir bakışta ayırt edilir kılar.
            Panel accent = new Panel();
            accent.Location = new Point(0, 10);
            accent.Size = new Size(3, card.Height - 20);
            accent.BackColor = WarningColor;
            card.Controls.Add(accent);

            Label icon = new Label();
            icon.AutoSize = false;
            icon.Location = new Point(14, 13);
            icon.Size = new Size(26, 24);
            icon.TextAlign = ContentAlignment.MiddleCenter;
            icon.Font = new Font("Segoe UI", 13f, FontStyle.Regular);
            icon.ForeColor = WarningColor;
            icon.Text = "⚠";
            card.Controls.Add(icon);

            Label heading = new Label();
            heading.AutoSize = false;
            heading.Location = new Point(44, 13);
            heading.Size = new Size(card.Width - 58, 20);
            heading.Font = new Font("Segoe UI Semibold", 9.5f, FontStyle.Bold);
            heading.ForeColor = WarningColor;
            heading.Text = "Minecraft satın alınmış olmalıdır";
            card.Controls.Add(heading);

            Label detail = new Label();
            detail.AutoSize = false;
            detail.Location = new Point(44, 35);
            detail.Size = new Size(card.Width - 58, 40);
            detail.Font = new Font("Segoe UI", 8.75f, FontStyle.Regular);
            detail.ForeColor = WarningText;
            detail.Text = "Bu giriş yöntemi yalnızca oyunu satın almış hesaplarla çalışır.\nSatın almadıysanız kullanıcı adıyla normal girişi kullanın.";
            card.Controls.Add(detail);

            return card.Bottom;
        }

        private void BuildSteps()
        {
            for (int i = 0; i < StepTitles.Length; i++)
            {
                StepIndicator indicator = new StepIndicator();
                indicator.Location = new Point(0, (i * StepRowHeight) + 5);
                indicator.Size = new Size(20, 20);
                indicator.BackColor = CardColor;
                _stepsPanel.Controls.Add(indicator);
                _indicators[i] = indicator;

                Label label = new Label();
                label.AutoSize = false;
                label.Location = new Point(32, (i * StepRowHeight) + 4);
                label.Size = new Size(_stepsPanel.Width - 32, 22);
                label.Font = new Font("Segoe UI", 9.5f, FontStyle.Regular);
                label.ForeColor = TextFaint;
                label.Text = StepTitles[i];
                _stepsPanel.Controls.Add(label);
                _stepLabels[i] = label;
            }
        }

        // --- Giriş akışı ---

        private async void OnSignInClick(object sender, EventArgs e)
        {
            if (!MicrosoftAuth.IsConfigured)
            {
                ShowMessage("Premium giriş bu başlatıcı sürümünde kullanılamıyor.", DangerColor);
                return;
            }

            _cancelledByUser = false;
            ResetSteps();
            BeginExpand();
            SetBusy(true);

            _cancellation = new CancellationTokenSource(SignInTimeout);

            try
            {
                Progress<PremiumAuthStage> progress = new Progress<PremiumAuthStage>(OnStageChanged);

                PremiumAccount account = await MicrosoftAuth.SignInAsync(progress, _cancellation.Token);

                PremiumSession.Save(account);
                Account = account;

                await ShowSuccessAsync(account);

                DialogResult = DialogResult.OK;
            }
            catch (OperationCanceledException)
            {
                MarkActiveStepFailed();
                SetBusy(false);

                ShowMessage(
                    _cancelledByUser
                        ? "Giriş iptal edildi."
                        : "Giriş süresi doldu. Tarayıcıdaki adımları tamamlayıp tekrar deneyin.",
                    _cancelledByUser ? TextMuted : WarningColor);
            }
            catch (PremiumAuthException exception)
            {
                MarkActiveStepFailed();
                SetBusy(false);
                ShowError(exception.Message, exception.ErrorCode, exception.Diagnostics);
            }
            catch (Exception unexpected)
            {
                MarkActiveStepFailed();
                SetBusy(false);
                ShowError(
                    "Giriş sırasında beklenmedik bir sorun oluştu. Lütfen tekrar deneyin.",
                    "GENEL",
                    unexpected.GetType().Name + ": " + unexpected.Message);
            }
            finally
            {
                if (_cancellation != null)
                {
                    _cancellation.Dispose();
                    _cancellation = null;
                }
            }
        }

        private void OnStageChanged(PremiumAuthStage stage)
        {
            switch (stage)
            {
                case PremiumAuthStage.WaitingBrowser:
                    SetActiveStep(0);
                    ShowMessage(
                        "Tarayıcınızda açılan sayfadan giriş yapın.\nSayfa açılmadıysa varsayılan tarayıcınızı denetleyin.",
                        TextMuted);
                    break;

                case PremiumAuthStage.ExchangingCode:
                    SetActiveStep(0);
                    ShowMessage("Giriş onaylandı, oturum hazırlanıyor…", TextMuted);
                    break;

                case PremiumAuthStage.XboxLive:
                    SetActiveStep(1);
                    ShowMessage("Hesabınız doğrulanıyor, bu birkaç saniye sürebilir…", TextMuted);
                    break;

                case PremiumAuthStage.Minecraft:
                    SetActiveStep(2);
                    break;

                case PremiumAuthStage.Profile:
                    SetActiveStep(3);
                    break;
            }
        }

        private async Task ShowSuccessAsync(PremiumAccount account)
        {
            for (int i = 0; i < _indicators.Length; i++)
            {
                _indicators[i].State = StepState.Done;
                _stepLabels[i].ForeColor = TextMuted;
            }

            _spinnerTimer.Stop();
            _activeStep = -1;

            _successLabel.Text = "Hoş geldin, " + account.Name + "!";
            _successLabel.Visible = true;
            _successLabel.BringToFront();

            ShowMessage("Oyuna çevrimiçi modda gireceksiniz.", SuccessColor);
            _cancelButton.Enabled = false;

            // Kullanıcı sonucu görebilsin diye pencere kısa bir an açık kalır.
            await Task.Delay(900);
        }

        private void OnCancelClick(object sender, EventArgs e)
        {
            if (_cancellation != null)
            {
                // Giriş sürüyor: yalnızca akış durdurulur, pencere açık kalır ki tekrar denenebilsin.
                _cancelledByUser = true;
                _cancellation.Cancel();
                return;
            }

            DialogResult = DialogResult.Cancel;
        }

        private void OnFormKeyDown(object sender, KeyEventArgs e)
        {
            if (e.KeyCode != Keys.Escape)
                return;

            e.Handled = true;
            OnCancelClick(sender, EventArgs.Empty);
        }

        // --- Görsel durum ---

        private void SetBusy(bool busy)
        {
            _signInButton.Enabled = !busy;
            _signInButton.Text = busy ? "Giriş bekleniyor…" : "Microsoft ile Giriş Yap";
            _cancelButton.Text = busy ? "İptal Et" : "Vazgeç";
            _cancelButton.Enabled = true;

            if (busy)
                _spinnerTimer.Start();
            else
                _spinnerTimer.Stop();
        }

        private void SetActiveStep(int index)
        {
            if (index < 0 || index >= _indicators.Length)
                return;

            for (int i = 0; i < _indicators.Length; i++)
            {
                if (i < index)
                {
                    _indicators[i].State = StepState.Done;
                    _stepLabels[i].ForeColor = TextMuted;
                }
                else if (i == index)
                {
                    _indicators[i].State = StepState.Active;
                    _stepLabels[i].ForeColor = TextPrimary;
                }
                else
                {
                    _indicators[i].State = StepState.Pending;
                    _stepLabels[i].ForeColor = TextFaint;
                }
            }

            _activeStep = index;
        }

        private void MarkActiveStepFailed()
        {
            if (_activeStep >= 0 && _activeStep < _indicators.Length)
            {
                _indicators[_activeStep].State = StepState.Failed;
                _stepLabels[_activeStep].ForeColor = DangerColor;
            }

            _spinnerTimer.Stop();
        }

        private void ResetSteps()
        {
            _successLabel.Visible = false;

            for (int i = 0; i < _indicators.Length; i++)
            {
                _indicators[i].State = StepState.Pending;
                _stepLabels[i].ForeColor = TextFaint;
            }

            _activeStep = -1;
            ShowMessage(IdleMessage, TextMuted);
            ShowErrorActions(false);
        }

        private void ShowMessage(string text, Color color)
        {
            _messageLabel.Text = text ?? string.Empty;
            _messageLabel.ForeColor = color;
        }

        /// <summary>
        /// Hata durumunu gösterir: açıklama, alt satırda hata kodu ve kodu paylaşma isteği,
        /// yanında kopyalama ile destek talebi butonları.
        /// </summary>
        private void ShowError(string message, string errorCode, string diagnostics)
        {
            _errorCode = string.IsNullOrEmpty(errorCode) ? "GENEL" : errorCode;
            _errorDiagnostics = string.IsNullOrEmpty(diagnostics) ? _errorCode : diagnostics;

            ShowMessage(message, DangerColor);

            _footerLabel.Text = "Hata kodu: " + _errorCode + " · Destek talebinizde bu kodu paylaşın.";
            _footerLabel.ForeColor = WarningColor;

            ShowErrorActions(true);
        }

        // Hata yokken kullanıcıyı yanıltmasın diye kopyalama ve destek butonları gizli durur;
        // alt satırı yalnızca hata anında üçe böleriz.
        private void ShowErrorActions(bool show)
        {
            _copyButton.Visible = show;
            _supportButton.Visible = show;
            _cancelButton.Width = show ? ActionWidth : ContentWidth;

            if (!show)
            {
                _errorCode = null;
                _errorDiagnostics = null;
                _copyButton.Text = "Kodu Kopyala";
                _footerLabel.Text = PrivacyNote;
                _footerLabel.ForeColor = TextFaint;
                _copyFeedbackTimer.Stop();
            }
        }

        private void OnCopyCodeClick(object sender, EventArgs e)
        {
            if (string.IsNullOrEmpty(_errorDiagnostics))
                return;

            try
            {
                Clipboard.SetText(_errorDiagnostics);

                _copyButton.Text = "Kopyalandı";
                _copyFeedbackTimer.Stop();
                _copyFeedbackTimer.Start();
            }
            catch (Exception)
            {
                ShowMessage("Hata kodu panoya kopyalanamadı: " + _errorCode, WarningColor);
            }
        }

        private void OnCopyFeedbackTick(object sender, EventArgs e)
        {
            _copyFeedbackTimer.Stop();
            _copyButton.Text = "Kodu Kopyala";
        }

        private void OnSupportClick(object sender, EventArgs e)
        {
            try
            {
                Process.Start(SupportUrl);
            }
            catch (Exception)
            {
                ShowMessage("Destek sayfası açılamadı. Adresi tarayıcınıza yazabilirsiniz:\n" + SupportUrl, WarningColor);
            }
        }

        private void OnSpinnerTick(object sender, EventArgs e)
        {
            if (_activeStep < 0 || _activeStep >= _indicators.Length)
                return;

            _indicators[_activeStep].Advance();
        }

        protected override void OnFormClosing(FormClosingEventArgs e)
        {
            // Pencere kapanırken bekleyen giriş akışı da bırakılır ki yerel dinleyici kapansın.
            if (_cancellation != null)
            {
                _cancelledByUser = true;

                try
                {
                    _cancellation.Cancel();
                }
                catch (ObjectDisposedException)
                {
                    // Akış zaten bitmiş.
                }
            }

            _spinnerTimer.Stop();
            _revealTimer.Stop();
            _copyFeedbackTimer.Stop();
            base.OnFormClosing(e);
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                if (_spinnerTimer != null)
                    _spinnerTimer.Dispose();

                if (_revealTimer != null)
                    _revealTimer.Dispose();

                if (_copyFeedbackTimer != null)
                    _copyFeedbackTimer.Dispose();

                if (_signInButton != null && _signInButton.Image != null)
                    _signInButton.Image.Dispose();
            }

            base.Dispose(disposing);
        }

        // --- Adım göstergesi ---

        private enum StepState
        {
            Pending,
            Active,
            Done,
            Failed,
        }

        /// <summary>
        /// Adım durumunu gösteren küçük daire. Yazı tipi simgeleri yerine GDI+ ile çizilir:
        /// böylece onay/çarpı işaretleri her Windows kurulumunda aynı görünür.
        /// </summary>
        private sealed class StepIndicator : Control
        {
            private StepState _state = StepState.Pending;
            private int _angle;

            public StepIndicator()
            {
                SetStyle(
                    ControlStyles.UserPaint |
                    ControlStyles.AllPaintingInWmPaint |
                    ControlStyles.OptimizedDoubleBuffer,
                    true);

                TabStop = false;
            }

            public StepState State
            {
                get { return _state; }
                set
                {
                    if (_state == value)
                        return;

                    _state = value;
                    _angle = 0;
                    Invalidate();
                }
            }

            /// <summary>Etkin adımın dönen halkasını bir adım ilerletir.</summary>
            public void Advance()
            {
                if (_state != StepState.Active)
                    return;

                _angle = (_angle + 24) % 360;
                Invalidate();
            }

            protected override void OnPaint(PaintEventArgs e)
            {
                Graphics graphics = e.Graphics;
                graphics.SmoothingMode = SmoothingMode.AntiAlias;

                Rectangle bounds = new Rectangle(1, 1, Width - 3, Height - 3);

                switch (_state)
                {
                    case StepState.Pending:
                    {
                        using (Pen ring = new Pen(Color.FromArgb(74, 81, 95), 2f))
                            graphics.DrawEllipse(ring, bounds);
                        break;
                    }

                    case StepState.Active:
                    {
                        using (Pen track = new Pen(Color.FromArgb(60, 66, 78), 2f))
                            graphics.DrawEllipse(track, bounds);

                        using (Pen arc = new Pen(WarningColor, 2f))
                        {
                            arc.StartCap = LineCap.Round;
                            arc.EndCap = LineCap.Round;
                            graphics.DrawArc(arc, bounds, _angle, 110);
                        }

                        break;
                    }

                    case StepState.Done:
                    {
                        using (SolidBrush fill = new SolidBrush(SuccessColor))
                            graphics.FillEllipse(fill, bounds);

                        using (Pen check = new Pen(Color.FromArgb(20, 24, 30), 2f))
                        {
                            check.StartCap = LineCap.Round;
                            check.EndCap = LineCap.Round;
                            check.LineJoin = LineJoin.Round;

                            graphics.DrawLines(check, new[]
                            {
                                new PointF(bounds.Left + (bounds.Width * 0.28f), bounds.Top + (bounds.Height * 0.52f)),
                                new PointF(bounds.Left + (bounds.Width * 0.44f), bounds.Top + (bounds.Height * 0.70f)),
                                new PointF(bounds.Left + (bounds.Width * 0.74f), bounds.Top + (bounds.Height * 0.32f)),
                            });
                        }

                        break;
                    }

                    case StepState.Failed:
                    {
                        using (SolidBrush fill = new SolidBrush(DangerColor))
                            graphics.FillEllipse(fill, bounds);

                        using (Pen cross = new Pen(Color.FromArgb(20, 24, 30), 2f))
                        {
                            cross.StartCap = LineCap.Round;
                            cross.EndCap = LineCap.Round;

                            float inset = bounds.Width * 0.32f;
                            graphics.DrawLine(cross,
                                bounds.Left + inset, bounds.Top + inset,
                                bounds.Right - inset, bounds.Bottom - inset);
                            graphics.DrawLine(cross,
                                bounds.Right - inset, bounds.Top + inset,
                                bounds.Left + inset, bounds.Bottom - inset);
                        }

                        break;
                    }
                }
            }
        }
    }
}
