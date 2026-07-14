using System;
using System.Collections.Generic;
using System.Drawing;
using System.Text;
using System.Windows.Forms;
using Guna.UI2.WinForms;

namespace Projects_Launcher.Afk
{
    /// <summary>
    /// Tek bir AFK oturumunun canlı konsolu: MCC'nin ürettiği günlüğü gösterir, durum rozetini
    /// günceller ve kullanıcının komut/sohbet göndermesini sağlar. Oturumun kendisini yönetmez;
    /// yalnızca var olan bir <see cref="AfkSession"/>'ı izler — pencere kapansa da oturum arka
    /// planda çalışmaya devam eder.
    ///
    /// Pencere modal değildir: AfkPanel her hesap için ayrı bir konsol açar, aynı anda birden
    /// fazla konsol yan yana kullanılabilir. Kenarlıksız olmasına rağmen kenarlarından tutulup
    /// yeniden boyutlandırılabilir (WM_NCHITTEST) ve görev çubuğunda kendi girdisiyle görünür.
    /// </summary>
    public class AfkConsoleForm : Form
    {
        private const int MaxLogLines = 500;

        // Görünür kutu sınırı bu kadar aştığında ham kayıttan topluca yeniden çizilir. Satır başına
        // kırpmak (eski Lines gidiş-dönüşü) her yeni satırda tüm metni kopyalıyordu; taşma payı
        // maliyeti tek seferlik bir yeniden çizime indirger.
        private const int TrimSlack = 100;

        // Kenarlıksız pencerenin yeniden boyutlandırma tutamacı: form Padding'i kadar içerideki
        // şerit çocuk kontrolsüz kalır ve fare mesajları forma düşer.
        private const int ResizeGrip = 6;

        private static readonly Color Background = Color.FromArgb(20, 22, 27);
        private static readonly Color Surface = Color.FromArgb(32, 36, 44);
        private static readonly Color BorderTone = Color.FromArgb(52, 58, 70);
        private static readonly Color TextTone = Color.FromArgb(245, 247, 250);
        private static readonly Color MutedTone = Color.FromArgb(152, 162, 179);

        private readonly AfkSession session;

        // Bu hesabın betiğinin (giriş komutları) gönderdiği komutlar; bir günlük satırının betik
        // komutu olup olmadığı, satırın bunlardan birini içerip içermediğine bakılarak belirlenir.
        private readonly List<string> scriptCommands;

        // Konsolun ham kaydı (gizli satırlar dahil). Görünür kutu bu listeden süzülerek üretilir,
        // böylece "betik komutlarını göster" açılıp kapandığında liste yeniden çizilebilir.
        private readonly List<ConsoleEntry> entries = new List<ConsoleEntry>();

        private RichTextBox logBox;
        private Guna2TextBox inputBox;
        private Guna2Button sendButton;
        private Guna2Button filterButton;
        private ToolTip filterTip;
        private Label statusLabel;

        // Görünür kutudaki satır sayısı; RenderAll ve AppendRendered günceller.
        private int visibleLineCount;

        private enum ConsoleEntryKind
        {
            Normal,     // her zaman görünür
            Script,     // yalnızca "betik komutlarını göster" açıkken görünür
            Sensitive,  // giriş/kayıt/parola: hiçbir koşulda görünmez
        }

        private sealed class ConsoleEntry
        {
            public DateTime Time;
            public string Text;
            public ConsoleEntryKind Kind;
        }

        public AfkConsoleForm(AfkSession session)
        {
            if (session == null) throw new ArgumentNullException("session");
            this.session = session;
            this.scriptCommands = BuildScriptCommands(session.Account);

            BuildLayout();
            LoadInitialLog();
            RefreshStatus();

            this.session.LogLine += OnLogLine;
            this.session.Changed += OnSessionChanged;
            AfkSettings.Changed += OnFilterChanged;
        }

        private void BuildLayout()
        {
            SuspendLayout();

            string nickname = (session.Account != null && !string.IsNullOrWhiteSpace(session.Account.Nickname))
                ? session.Account.Nickname
                : "Hesap";

            FormBorderStyle = FormBorderStyle.None;
            StartPosition = FormStartPosition.Manual; // çağıran (AfkPanel) basamaklı yerleştirir
            ClientSize = new Size(760, 520);
            MinimumSize = new Size(620, 400);
            BackColor = Background;
            Padding = new Padding(ResizeGrip); // kenarlarda boyutlandırma şeridi bırakır
            ShowInTaskbar = true;
            MinimizeBox = true;
            MaximizeBox = false;
            DoubleBuffered = true;
            Text = "Konsol — " + nickname;

            Guna2Elipse formElipse = new Guna2Elipse();
            formElipse.TargetControl = this;
            formElipse.BorderRadius = 12;

            // --- Üst başlık şeridi ---
            Panel titleBar = new Panel();
            titleBar.Dock = DockStyle.Top;
            titleBar.Height = 48;
            titleBar.BackColor = Surface;

            Label titleLabel = new Label();
            titleLabel.AutoSize = false;
            titleLabel.AutoEllipsis = true;
            titleLabel.Location = new Point(16, 10);
            titleLabel.Size = new Size(240, 28);
            titleLabel.TextAlign = ContentAlignment.MiddleLeft;
            titleLabel.Font = new Font("Segoe UI", 11f, FontStyle.Bold);
            titleLabel.ForeColor = TextTone;
            titleLabel.BackColor = Color.Transparent;
            titleLabel.Text = "Konsol — " + nickname;
            titleBar.Controls.Add(titleLabel);

            Guna2Button closeButton = new Guna2Button();
            closeButton.Text = "✕";
            closeButton.Font = new Font("Segoe UI", 9f, FontStyle.Bold);
            closeButton.ForeColor = MutedTone;
            closeButton.FillColor = BorderTone;
            closeButton.BackColor = Surface; // yuvarlak köşeler şerit rengine karışsın
            closeButton.BorderRadius = 8;
            closeButton.Size = new Size(32, 32);
            closeButton.Location = new Point(titleBar.Width - 12 - 32, 8);
            closeButton.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            closeButton.Cursor = Cursors.Hand;
            closeButton.HoverState.FillColor = Color.FromArgb(239, 68, 68);
            closeButton.HoverState.ForeColor = Color.White;
            closeButton.Click += delegate { Close(); };
            titleBar.Controls.Add(closeButton);

            statusLabel = new Label();
            statusLabel.AutoSize = false;
            statusLabel.Size = new Size(380, 28);
            statusLabel.Location = new Point(closeButton.Left - 10 - 380, 10);
            statusLabel.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            statusLabel.Font = new Font("Segoe UI", 9f, FontStyle.Regular);
            statusLabel.TextAlign = ContentAlignment.MiddleRight;
            statusLabel.ForeColor = MutedTone;
            statusLabel.BackColor = Color.Transparent;
            titleBar.Controls.Add(statusLabel);

            // Betik komutlarını göster/gizle anahtarı. Genel bir ayardır: açık tüm konsollar aynı değeri paylaşır.
            filterButton = new Guna2Button();
            filterButton.Font = new Font("Segoe UI Emoji", 10f);
            filterButton.ForeColor = MutedTone;
            filterButton.FillColor = BorderTone;
            filterButton.BackColor = Surface; // yuvarlak köşeler şerit rengine karışsın
            filterButton.BorderRadius = 8;
            filterButton.Size = new Size(32, 32);
            filterButton.Location = new Point(statusLabel.Left - 8 - 32, 8);
            filterButton.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            filterButton.Cursor = Cursors.Hand;
            filterButton.HoverState.FillColor = Color.FromArgb(60, 66, 78);
            filterButton.Click += delegate { AfkSettings.ShowScriptCommands = !AfkSettings.ShowScriptCommands; };
            titleBar.Controls.Add(filterButton);

            filterTip = new ToolTip { UseFading = true, ShowAlways = true };
            UpdateFilterButton();

            // Pencerenin kendi başlık çubuğu yok; tüm üst şerit sürükleme tutamacı olarak davranır.
            Guna2DragControl dragControl = new Guna2DragControl();
            dragControl.TargetControl = titleBar;

            // --- Orta: günlük görünümü ---
            Panel logPanel = new Panel();
            logPanel.Dock = DockStyle.Fill;
            logPanel.BackColor = Background;
            logPanel.Padding = new Padding(10, 10, 10, 10);

            logBox = new RichTextBox();
            logBox.Dock = DockStyle.Fill;
            logBox.BackColor = Color.FromArgb(14, 16, 20);
            logBox.ForeColor = MutedTone;
            logBox.Font = new Font("Consolas", 9f);
            logBox.ReadOnly = true;
            logBox.BorderStyle = BorderStyle.None;
            logBox.WordWrap = true;
            logBox.ScrollBars = RichTextBoxScrollBars.Vertical;
            logBox.DetectUrls = false;
            logBox.TabStop = false;
            logPanel.Controls.Add(logBox);

            // --- Alt: komut/sohbet girişi ---
            Panel bottomBar = new Panel();
            bottomBar.Dock = DockStyle.Bottom;
            bottomBar.Height = 62;
            bottomBar.BackColor = Surface;

            sendButton = new Guna2Button();
            sendButton.Text = "Gönder";
            sendButton.Font = new Font("Segoe UI", 9.5f, FontStyle.Bold);
            sendButton.ForeColor = Color.White;
            sendButton.FillColor = Color.FromArgb(94, 148, 255);
            sendButton.BackColor = Surface; // yuvarlak köşeler şerit rengine karışsın
            sendButton.BorderRadius = 8;
            sendButton.Size = new Size(104, 36);
            sendButton.Location = new Point(bottomBar.Width - 16 - 104, 13);
            sendButton.Anchor = AnchorStyles.Top | AnchorStyles.Right;
            sendButton.Cursor = Cursors.Hand;
            sendButton.HoverState.FillColor = Color.FromArgb(59, 110, 220);
            sendButton.DisabledState.FillColor = Color.FromArgb(44, 48, 58);
            sendButton.DisabledState.ForeColor = Color.FromArgb(96, 102, 116);
            sendButton.Click += delegate { SendCurrentInput(); };
            bottomBar.Controls.Add(sendButton);

            inputBox = new Guna2TextBox();
            inputBox.Location = new Point(16, 13);
            inputBox.Size = new Size(sendButton.Left - 12 - 16, 36);
            inputBox.Anchor = AnchorStyles.Top | AnchorStyles.Left | AnchorStyles.Right;
            inputBox.PlaceholderText = "Komut ya da sohbet mesajı yazın…";
            inputBox.FillColor = Background;
            inputBox.BackColor = Surface; // yuvarlak köşeler şerit rengine karışsın
            inputBox.BorderColor = BorderTone;
            inputBox.ForeColor = TextTone;
            inputBox.PlaceholderForeColor = MutedTone;
            inputBox.BorderRadius = 8;
            inputBox.Font = new Font("Segoe UI", 9.5f);
            AfkUi.AttachKeyDown(inputBox, InputBox_KeyDown);
            bottomBar.Controls.Add(inputBox);

            // Yerleşim motoru yığını sondan başa işler: Fill olan panel en başta eklenmeli ki
            // en son yerleşsin ve şeritlerden arta kalan alanı alsın. Aksi halde günlük kutusu
            // başlık ve giriş şeritlerinin altına taşar.
            Controls.Add(logPanel);
            Controls.Add(bottomBar);
            Controls.Add(titleBar);

            ResumeLayout(false);
        }

        /// <summary>
        /// Kenarlıksız formu kenarlarından boyutlandırılabilir yapar: Padding ile açılan
        /// çerçeve şeridi üzerindeki fare vuruşları ilgili tutamaca çevrilir.
        /// </summary>
        protected override void WndProc(ref Message m)
        {
            const int WM_NCHITTEST = 0x84;
            const int HTCLIENT = 1;
            const int HTLEFT = 10;
            const int HTRIGHT = 11;
            const int HTTOP = 12;
            const int HTTOPLEFT = 13;
            const int HTTOPRIGHT = 14;
            const int HTBOTTOM = 15;
            const int HTBOTTOMLEFT = 16;
            const int HTBOTTOMRIGHT = 17;

            if (m.Msg == WM_NCHITTEST)
            {
                base.WndProc(ref m);

                if ((int)m.Result != HTCLIENT)
                    return;

                // Çoklu ekranda negatif koordinatlar için işaretli 16 bit ayrıştırma gerekir.
                int screenX = unchecked((short)(long)m.LParam);
                int screenY = unchecked((short)((long)m.LParam >> 16));
                Point p = PointToClient(new Point(screenX, screenY));

                bool left = p.X < ResizeGrip;
                bool right = p.X >= ClientSize.Width - ResizeGrip;
                bool top = p.Y < ResizeGrip;
                bool bottom = p.Y >= ClientSize.Height - ResizeGrip;

                int hit = HTCLIENT;
                if (top && left) hit = HTTOPLEFT;
                else if (top && right) hit = HTTOPRIGHT;
                else if (bottom && left) hit = HTBOTTOMLEFT;
                else if (bottom && right) hit = HTBOTTOMRIGHT;
                else if (left) hit = HTLEFT;
                else if (right) hit = HTRIGHT;
                else if (top) hit = HTTOP;
                else if (bottom) hit = HTBOTTOM;

                m.Result = (IntPtr)hit;
                return;
            }

            base.WndProc(ref m);
        }

        private void InputBox_KeyDown(object sender, KeyEventArgs e)
        {
            if (e.KeyCode != Keys.Enter)
                return;

            // Tek satırlık kutuda Enter'ın "ding" sesi çıkarmasını / satır eklemesini engeller.
            e.SuppressKeyPress = true;
            SendCurrentInput();
        }

        private void SendCurrentInput()
        {
            if (!session.IsRunning)
                return;

            string text = inputBox.Text;
            if (string.IsNullOrWhiteSpace(text))
                return;

            session.SendCommand(text);
            // Kullanıcının elle yazdığı satır her zaman görünür: kendi girdisini konsolda görmeli.
            AddEntry(DateTime.Now, "> " + text, ConsoleEntryKind.Normal);
            inputBox.Text = string.Empty;
        }

        private void LoadInitialLog()
        {
            foreach (AfkLogEntry item in session.LogSnapshot())
                entries.Add(new ConsoleEntry { Time = item.Time, Text = item.Text, Kind = Classify(item.Text) });

            TrimEntries();
            RenderAll();
        }

        /// <summary>Yeni bir satırı ham kayda ekler ve süzgeçten geçiyorsa görünür kutuya yazar.</summary>
        private void AddEntry(DateTime time, string text, ConsoleEntryKind kind)
        {
            ConsoleEntry entry = new ConsoleEntry { Time = time, Text = text, Kind = kind };
            entries.Add(entry);
            TrimEntries();

            if (IsVisible(entry))
                AppendRendered(entry);
        }

        private void TrimEntries()
        {
            if (entries.Count > MaxLogLines)
                entries.RemoveRange(0, entries.Count - MaxLogLines);
        }

        private void AppendRendered(ConsoleEntry entry)
        {
            // Girdi zaten entries listesine eklendi; kutu taşma payını doldurduysa liste
            // (MaxLogLines ile sınırlı) üzerinden topluca yeniden çizmek yeterli.
            if (visibleLineCount >= MaxLogLines + TrimSlack)
            {
                RenderAll();
                return;
            }

            if (logBox.TextLength > 0)
                logBox.AppendText(Environment.NewLine);
            logBox.AppendText(Render(entry));
            visibleLineCount++;

            logBox.SelectionStart = logBox.TextLength;
            logBox.ScrollToCaret();
        }

        /// <summary>Görünür kutuyu ham kayıttan yeniden üretir (süzgeç değişince çağrılır).</summary>
        private void RenderAll()
        {
            List<ConsoleEntry> visible = new List<ConsoleEntry>(entries.Count);
            foreach (ConsoleEntry entry in entries)
            {
                if (IsVisible(entry))
                    visible.Add(entry);
            }

            int start = Math.Max(0, visible.Count - MaxLogLines);
            StringBuilder sb = new StringBuilder();
            for (int i = start; i < visible.Count; i++)
            {
                if (sb.Length > 0)
                    sb.Append(Environment.NewLine);
                sb.Append(Render(visible[i]));
            }

            logBox.Text = sb.ToString();
            visibleLineCount = visible.Count - start;
            logBox.SelectionStart = logBox.TextLength;
            logBox.ScrollToCaret();
        }

        // Her satırın başına saat damgası eklenir: [SS:DD:ss] metin. Hesabın "Sohbet Günlüğü"
        // ayarında tarih açıksa saatin önüne gün de yazılır: [11.07.2026 14:32:05] metin.
        private string Render(ConsoleEntry entry)
        {
            ChatLogOptions chatLog = session.Account != null ? session.Account.ChatLog : null;
            string format = chatLog != null && chatLog.ShowDate ? "dd.MM.yyyy HH:mm:ss" : "HH:mm:ss";
            return "[" + entry.Time.ToString(format) + "] " + entry.Text;
        }

        private static bool IsVisible(ConsoleEntry entry)
        {
            if (entry.Kind == ConsoleEntryKind.Sensitive)
                return false;
            if (entry.Kind == ConsoleEntryKind.Script && !AfkSettings.ShowScriptCommands)
                return false;
            return true;
        }

        /// <summary>Bir satırın betik komutu / hassas komut / sıradan oluşunu belirler.</summary>
        private ConsoleEntryKind Classify(string text)
        {
            if (string.IsNullOrEmpty(text) || scriptCommands.Count == 0)
                return ConsoleEntryKind.Normal;

            bool isScript = false;
            foreach (string command in scriptCommands)
            {
                if (text.IndexOf(command, StringComparison.OrdinalIgnoreCase) < 0)
                    continue;

                isScript = true;
                // Hassas komut satırı her koşulda gizlenir; başka satır aramaya gerek yok.
                if (AfkScriptCommands.IsSensitive(command))
                    return ConsoleEntryKind.Sensitive;
            }

            return isScript ? ConsoleEntryKind.Script : ConsoleEntryKind.Normal;
        }

        private static List<string> BuildScriptCommands(AfkAccount account)
        {
            List<string> list = new List<string>();
            if (account == null || account.LoginCommands == null || account.LoginCommands.Commands == null)
                return list;

            foreach (string command in account.LoginCommands.Commands)
            {
                if (string.IsNullOrWhiteSpace(command))
                    continue;

                string trimmed = command.Trim();
                if (!list.Contains(trimmed))
                    list.Add(trimmed);
            }

            return list;
        }

        private void UpdateFilterButton()
        {
            bool shown = AfkSettings.ShowScriptCommands;
            filterButton.Text = shown ? "👁" : "🙈";

            if (filterTip != null)
            {
                filterTip.SetToolTip(filterButton, shown
                    ? "Betik komutları gösteriliyor · giriş/parola komutları yine gizli. Gizlemek için tıklayın."
                    : "Betik komutları gizli. Göstermek için tıklayın.");
            }
        }

        // Süzgeç başka bir konsolda da değişmiş olabilir; kutuyu yeni ayarla yeniden çizeriz.
        private void OnFilterChanged(object sender, EventArgs e)
        {
            SafeInvoke(delegate
            {
                UpdateFilterButton();
                RenderAll();
            });
        }

        // AfkSession.LogLine iş parçacığı havuzunda tetiklenir; arayüze BeginInvoke ile marshal edilir.
        private void OnLogLine(object sender, AfkLogEntry line)
        {
            SafeInvoke(delegate { AddEntry(line.Time, line.Text, Classify(line.Text)); });
        }

        // AfkSession.Changed de aynı şekilde iş parçacığı havuzunda tetiklenir.
        private void OnSessionChanged(object sender, EventArgs e)
        {
            SafeInvoke(delegate { RefreshStatus(); });
        }

        private void SafeInvoke(MethodInvoker action)
        {
            if (IsDisposed || !IsHandleCreated)
                return;

            try
            {
                BeginInvoke(action);
            }
            catch (Exception)
            {
                // Olay tetiklenirken pencere tam o anda kapanmış olabilir; yok sayılır.
            }
        }

        private void RefreshStatus()
        {
            AfkState state = session.State;

            statusLabel.Text = BuildStatusText(state);
            statusLabel.ForeColor = ColorForState(state);

            bool canSend = session.IsRunning;
            inputBox.Enabled = canSend;
            sendButton.Enabled = canSend;
        }

        private string BuildStatusText(AfkState state)
        {
            StringBuilder sb = new StringBuilder();
            sb.Append(DescribeState(state));

            bool hasExtra = session.Health.HasValue || session.Food.HasValue || session.OnlinePlayers.HasValue;
            if (!hasExtra)
                return sb.ToString();

            sb.Append("   ");
            bool appended = false;

            if (session.Health.HasValue)
            {
                sb.Append("❤ ").Append(session.Health.Value.ToString("0.#"));
                appended = true;
            }

            if (session.Food.HasValue)
            {
                if (appended) sb.Append(" · ");
                sb.Append("🍗 ").Append(session.Food.Value);
                appended = true;
            }

            if (session.OnlinePlayers.HasValue)
            {
                if (appended) sb.Append(" · ");
                sb.Append(session.OnlinePlayers.Value).Append(" oyuncu");
            }

            return sb.ToString();
        }

        private static string DescribeState(AfkState state)
        {
            switch (state)
            {
                case AfkState.Connected: return "Bağlı";
                case AfkState.Connecting: return "Bağlanıyor";
                case AfkState.Reconnecting: return "Yeniden bağlanıyor";
                case AfkState.Starting: return "Başlatılıyor";
                case AfkState.Stopping: return "Kapatılıyor";
                case AfkState.Error: return "Hata";
                default: return "Bağlı değil";
            }
        }

        private static Color ColorForState(AfkState state)
        {
            switch (state)
            {
                case AfkState.Connected:
                    return Color.FromArgb(34, 197, 94);
                case AfkState.Connecting:
                case AfkState.Reconnecting:
                case AfkState.Starting:
                case AfkState.Stopping:
                    return Color.FromArgb(245, 158, 11);
                case AfkState.Error:
                    return Color.FromArgb(239, 68, 68);
                default:
                    return Color.FromArgb(152, 162, 179);
            }
        }

        protected override void OnFormClosed(FormClosedEventArgs e)
        {
            // Oturum pencere kapansa da yaşamaya devam eder; olay aboneliklerini kesinlikle kaldır.
            session.LogLine -= OnLogLine;
            session.Changed -= OnSessionChanged;
            AfkSettings.Changed -= OnFilterChanged;
            base.OnFormClosed(e);
        }

        protected override void Dispose(bool disposing)
        {
            // ToolTip, Controls koleksiyonunda olmadığı için formla birlikte kendiliğinden yok edilmez.
            if (disposing && filterTip != null)
                filterTip.Dispose();

            base.Dispose(disposing);
        }
    }
}
