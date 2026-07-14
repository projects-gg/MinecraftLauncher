using System;
using System.Collections.Generic;
using System.Drawing;
using System.Globalization;
using System.Windows.Forms;
using Guna.UI2.WinForms;

namespace Projects_Launcher.Afk
{
    /// <summary>
    /// Tek bir AFK hesabının tüm davranış ayarlarını düzenleyen pencere.
    /// Çağıran taraf hesabın Clone() edilmiş bir kopyasını verir; bu form doğrudan o kopya
    /// üzerinde çalışır. "Kaydet" doğrulama geçerse alanları kopyaya yazar ve DialogResult.OK
    /// döner; "Vazgeç" hiçbir şeyi değiştirmeden DialogResult.Cancel ile kapanır.
    ///
    /// Yerleşim: soldaki gezinme her bölümü ayrı bir sayfada açar. Sayfa içeriği yukarıdan
    /// aşağıya bir imleçle (ref y) dizilir ve etiket yükseklikleri TextRenderer ile ölçülür;
    /// böylece hiçbir kontrol bir diğerinin üzerine binemez.
    /// </summary>
    public class AfkAccountSettingsForm : Form
    {
        private static readonly Color Background = Color.FromArgb(20, 22, 27);
        private static readonly Color NavBackground = Color.FromArgb(16, 18, 23);
        private static readonly Color NavSelected = Color.FromArgb(38, 43, 53);
        private static readonly Color NavHover = Color.FromArgb(27, 30, 37);
        private static readonly Color FieldFill = Color.FromArgb(28, 32, 40);
        private static readonly Color BorderTone = Color.FromArgb(52, 58, 70);
        private static readonly Color TextTone = Color.FromArgb(245, 247, 250);
        private static readonly Color MutedTone = Color.FromArgb(152, 162, 179);
        private static readonly Color GreenTone = Color.FromArgb(34, 197, 94);
        private static readonly Color RedTone = Color.FromArgb(239, 68, 68);
        private static readonly Color BlueTone = Color.FromArgb(94, 148, 255);
        private static readonly Color GrayFill = Color.FromArgb(71, 76, 88);

        private const int FormWidth = 780;
        private const int FormHeight = 640;
        private const int TitleBarHeight = 48;
        private const int BottomBarHeight = 60;
        private const int NavWidth = 190;
        private const int PageMarginX = 24;

        // Sayfada dikey kaydırma çubuğu belirse bile alanlar çubuğun altında kalmasın.
        private const int PageContentWidth = FormWidth - NavWidth - PageMarginX * 2 - 18;

        private readonly AfkAccount account;

        // Pencere çerçevesi: FormBorderStyle.None olduğundan yuvarlama ve taşıma bu iki
        // bileşenle sağlanır.
        private readonly Guna2Elipse formElipse;
        private Guna2DragControl dragControl;

        private Guna2Panel titleBar;
        private Panel navPanel;
        private Panel bottomBar;
        private Guna2Button cancelButton;
        private Guna2Button saveButton;

        private readonly List<Guna2Button> navButtons = new List<Guna2Button>();
        private readonly List<Guna2Panel> pages = new List<Guna2Panel>();
        private int selectedPageIndex = -1;

        // --- Sunucu ---
        private Guna2TextBox serverHostBox;
        private Guna2NumericUpDown serverPortUpDown;
        private Guna2ToggleSwitch autoStartToggle;

        // --- Giriş Komutları ---
        private Guna2ToggleSwitch loginCommandsToggle;
        private Guna2TextBox loginCommandsBox;
        private Guna2ToggleSwitch loginOnlyFirstToggle;
        private Guna2NumericUpDown loginDelayUpDown;

        // --- Zamanlayıcı (Scheduler) ---
        private Guna2ToggleSwitch schedulerToggle;
        private Guna2DataGridView schedulerGrid;
        private Guna2Button addTaskButton;
        private Guna2Button deleteTaskButton;

        // --- AFK Engelleme (AntiAFK) ---
        private Guna2ToggleSwitch antiAfkToggle;
        private Guna2NumericUpDown antiAfkDelayMinUpDown;
        private Guna2NumericUpDown antiAfkDelayMaxUpDown;
        private Guna2TextBox antiAfkCommandBox;
        private Guna2ToggleSwitch antiAfkSneakToggle;
        private Guna2ToggleSwitch antiAfkTerrainToggle;
        private Guna2NumericUpDown antiAfkWalkRangeUpDown;
        private Guna2NumericUpDown antiAfkWalkRetriesUpDown;

        // --- Yeniden Bağlanma (AutoRelog) ---
        private Guna2ToggleSwitch autoRelogToggle;
        private Guna2NumericUpDown autoRelogDelayMinUpDown;
        private Guna2NumericUpDown autoRelogDelayMaxUpDown;
        private Guna2ToggleSwitch autoRelogUnlimitedToggle;
        private Guna2NumericUpDown autoRelogRetriesUpDown;
        private Guna2ToggleSwitch autoRelogIgnoreKickToggle;
        private Guna2TextBox autoRelogKickMessagesBox;

        // --- Otomatik Yemek (AutoEat) ---
        private Guna2ToggleSwitch autoEatToggle;
        private Guna2NumericUpDown autoEatThresholdUpDown;

        // --- Otomatik Yanıt (AutoRespond) ---
        private Guna2ToggleSwitch autoRespondToggle;
        private Guna2ToggleSwitch autoRespondSoundToggle;
        private Guna2ToggleSwitch autoRespondMatchColorsToggle;
        private Guna2DataGridView autoRespondGrid;
        private Guna2Button addRespondRowButton;
        private Guna2Button deleteRespondRowButton;

        // --- Sohbet Günlüğü (ChatLog) ---
        private Guna2ToggleSwitch chatLogToggle;
        private Guna2ToggleSwitch chatLogDateTimeToggle;
        private Guna2ToggleSwitch chatLogShowDateToggle;
        private Guna2ComboBox chatLogFilterCombo;

        // --- Uyarılar (Alerts) ---
        private Guna2ToggleSwitch alertsToggle;
        private Guna2ToggleSwitch alertsBeepToggle;
        private Guna2ToggleSwitch alertsTriggerWordsToggle;
        private Guna2ToggleSwitch alertsLogToFileToggle;
        private Guna2TextBox alertsMatchesBox;
        private Guna2TextBox alertsExcludesBox;

        public AfkAccountSettingsForm(AfkAccount account)
        {
            if (account == null) throw new ArgumentNullException("account");
            this.account = account;

            SuspendLayout();

            FormBorderStyle = FormBorderStyle.None;
            StartPosition = FormStartPosition.CenterParent;
            ShowInTaskbar = false;
            DoubleBuffered = true;
            KeyPreview = true;
            Size = new Size(FormWidth, FormHeight);
            BackColor = Background;
            Font = new Font("Segoe UI", 9f);
            Text = "AFK Ayarları";

            formElipse = new Guna2Elipse();
            formElipse.TargetControl = this;
            formElipse.BorderRadius = 12;

            BuildChrome();
            BuildPages();

            LoadFromAccount();
            SelectPage(0);

            KeyDown += OnFormKeyDown;

            ResumeLayout(false);
        }

        /// <summary>Kaydet'e basılırsa güncellenmiş hâli; aksi hâlde çağıranın verdiği kopya.</summary>
        public AfkAccount Account
        {
            get { return account; }
        }

        // --- Çerçeve: başlık şeridi / sol gezinme / alt şerit ---

        private void BuildChrome()
        {
            titleBar = new Guna2Panel();
            titleBar.Location = new Point(0, 0);
            titleBar.Size = new Size(FormWidth, TitleBarHeight);
            titleBar.BackColor = Background;
            titleBar.FillColor = Background;
            Controls.Add(titleBar);

            Label titleLabel = new Label();
            titleLabel.AutoSize = false;
            titleLabel.AutoEllipsis = true;
            titleLabel.Location = new Point(20, 13);
            titleLabel.Size = new Size(FormWidth - 20 - 70, 22);
            titleLabel.Font = new Font("Segoe UI", 11f, FontStyle.Bold);
            titleLabel.ForeColor = TextTone;
            titleLabel.BackColor = Color.Transparent;
            titleLabel.Text = "AFK Ayarları — " + account.Nickname;
            titleBar.Controls.Add(titleLabel);

            Guna2Button closeXButton = new Guna2Button();
            closeXButton.Size = new Size(38, 30);
            closeXButton.Location = new Point(FormWidth - 16 - 38, 9);
            closeXButton.BorderRadius = 6;
            closeXButton.FillColor = Background;
            closeXButton.BackColor = Background; // yuvarlak köşeler şerit rengine karışsın
            closeXButton.ForeColor = MutedTone;
            closeXButton.Font = new Font("Segoe UI", 10.5f, FontStyle.Bold);
            closeXButton.Cursor = Cursors.Hand;
            closeXButton.Text = "✕";
            closeXButton.HoverState.FillColor = RedTone;
            closeXButton.HoverState.ForeColor = Color.White;
            closeXButton.Click += OnCancelClicked;
            titleBar.Controls.Add(closeXButton);

            dragControl = new Guna2DragControl();
            dragControl.TargetControl = titleBar;

            int middleHeight = FormHeight - TitleBarHeight - BottomBarHeight;

            navPanel = new Panel();
            navPanel.Location = new Point(0, TitleBarHeight);
            navPanel.Size = new Size(NavWidth, middleHeight);
            navPanel.BackColor = NavBackground;
            Controls.Add(navPanel);

            // İnce ayırıcı çizgiler: başlık altı, gezinme sağı ve alt şerit üstü.
            AddLine(0, TitleBarHeight - 1, FormWidth, 1);
            AddLine(NavWidth - 1, TitleBarHeight, 1, middleHeight);
            AddLine(0, FormHeight - BottomBarHeight, FormWidth, 1);

            bottomBar = new Panel();
            bottomBar.Location = new Point(0, FormHeight - BottomBarHeight + 1);
            bottomBar.Size = new Size(FormWidth, BottomBarHeight - 1);
            bottomBar.BackColor = Background;
            Controls.Add(bottomBar);

            Label hintLabel = new Label();
            hintLabel.AutoSize = false;
            hintLabel.Location = new Point(20, 13);
            hintLabel.Size = new Size(FormWidth - 20 - 270, 32);
            hintLabel.Font = new Font("Segoe UI", 8.25f);
            hintLabel.ForeColor = MutedTone;
            hintLabel.BackColor = Color.Transparent;
            hintLabel.TextAlign = ContentAlignment.MiddleLeft;
            hintLabel.Text = "Bağlı bir hesapta değişiklikler, bağlantı yeniden kurulunca geçerli olur.";
            bottomBar.Controls.Add(hintLabel);

            saveButton = new Guna2Button();
            saveButton.Size = new Size(120, 36);
            saveButton.Location = new Point(FormWidth - 20 - 120, 11);
            saveButton.BorderRadius = 8;
            saveButton.FillColor = GreenTone;
            saveButton.BackColor = Background;
            saveButton.ForeColor = Color.White;
            saveButton.Font = new Font("Segoe UI", 9.5f, FontStyle.Bold);
            saveButton.Cursor = Cursors.Hand;
            saveButton.Text = "Kaydet";
            saveButton.HoverState.FillColor = ControlPaint.Light(GreenTone, 0.15f);
            saveButton.Click += OnSaveClicked;
            bottomBar.Controls.Add(saveButton);

            cancelButton = new Guna2Button();
            cancelButton.Size = new Size(104, 36);
            cancelButton.Location = new Point(saveButton.Left - 12 - 104, 11);
            cancelButton.BorderRadius = 8;
            cancelButton.FillColor = GrayFill;
            cancelButton.BackColor = Background;
            cancelButton.ForeColor = Color.White;
            cancelButton.Font = new Font("Segoe UI", 9.5f, FontStyle.Bold);
            cancelButton.Cursor = Cursors.Hand;
            cancelButton.Text = "Vazgeç";
            cancelButton.HoverState.FillColor = ControlPaint.Light(GrayFill, 0.15f);
            cancelButton.Click += OnCancelClicked;
            bottomBar.Controls.Add(cancelButton);
        }

        private void AddLine(int x, int y, int width, int height)
        {
            Panel line = new Panel();
            line.Location = new Point(x, y);
            line.Size = new Size(width, height);
            line.BackColor = Color.FromArgb(40, 45, 55);
            Controls.Add(line);
            line.BringToFront();
        }

        private void OnCancelClicked(object sender, EventArgs e)
        {
            DialogResult = DialogResult.Cancel;
            Close();
        }

        private void OnFormKeyDown(object sender, KeyEventArgs e)
        {
            if (e.KeyCode != Keys.Escape)
                return;

            // Izgara hücresi düzenlenirken Escape önce düzenlemeyi iptal etmeli, pencereyi değil.
            if ((autoRespondGrid != null && autoRespondGrid.IsCurrentCellInEditMode) ||
                (schedulerGrid != null && schedulerGrid.IsCurrentCellInEditMode))
                return;

            e.Handled = true;
            OnCancelClicked(sender, EventArgs.Empty);
        }

        // --- Gezinme ve sayfa altyapısı ---

        private Guna2Panel NewPage(string navText, string pageTitle, string pageDescription, out int y)
        {
            int index = pages.Count;

            Guna2Button navButton = new Guna2Button();
            navButton.Size = new Size(NavWidth - 16, 36);
            navButton.Location = new Point(8, 10 + index * 42);
            navButton.BorderRadius = 8;
            navButton.FillColor = NavBackground;
            navButton.BackColor = NavBackground; // yuvarlak köşeler gezinme zeminine karışsın
            navButton.ForeColor = MutedTone;
            navButton.Font = new Font("Segoe UI", 9f);
            navButton.Cursor = Cursors.Hand;
            navButton.Text = navText;
            navButton.TextAlign = HorizontalAlignment.Left;
            navButton.TextOffset = new Point(8, 0);
            navButton.HoverState.FillColor = NavHover;
            navButton.Click += delegate { SelectPage(index); };
            navPanel.Controls.Add(navButton);
            navButtons.Add(navButton);

            Guna2Panel page = new Guna2Panel();
            page.Location = new Point(NavWidth, TitleBarHeight);
            page.Size = new Size(FormWidth - NavWidth, FormHeight - TitleBarHeight - BottomBarHeight);
            page.BackColor = Background;
            page.FillColor = Background;
            page.AutoScroll = true;
            page.Visible = false;
            Controls.Add(page);
            pages.Add(page);

            y = 18;

            Label title = MakeWrapLabel(page, pageTitle, new Font("Segoe UI", 12.5f, FontStyle.Bold),
                TextTone, PageMarginX, y, PageContentWidth);
            y += title.Height + 4;

            if (!string.IsNullOrEmpty(pageDescription))
            {
                Label desc = MakeWrapLabel(page, pageDescription, new Font("Segoe UI", 8.75f),
                    MutedTone, PageMarginX, y, PageContentWidth);
                y += desc.Height + 6;
            }

            Panel line = new Panel();
            line.Location = new Point(PageMarginX, y + 4);
            line.Size = new Size(PageContentWidth, 1);
            line.BackColor = BorderTone;
            page.Controls.Add(line);
            y += 20;

            return page;
        }

        private void SelectPage(int index)
        {
            if (index == selectedPageIndex || index < 0 || index >= pages.Count)
                return;

            selectedPageIndex = index;

            for (int i = 0; i < pages.Count; i++)
            {
                bool selected = i == index;
                pages[i].Visible = selected;
                navButtons[i].FillColor = selected ? NavSelected : NavBackground;
                navButtons[i].ForeColor = selected ? TextTone : MutedTone;
                navButtons[i].Font = new Font("Segoe UI", 9f, selected ? FontStyle.Bold : FontStyle.Regular);
            }
        }

        // --- Satır üreticileri: her satır y imlecini kendi ölçüsü kadar ilerletir ---

        /// <summary>Verilen genişliğe sarılan, yüksekliği ölçülerek sabitlenen etiket.</summary>
        private static Label MakeWrapLabel(Control parent, string text, Font font, Color color, int x, int y, int width)
        {
            Size measured = TextRenderer.MeasureText(text, font,
                new Size(width, int.MaxValue), TextFormatFlags.WordBreak);

            Label label = new Label();
            label.AutoSize = false;
            label.Location = new Point(x, y);
            label.Size = new Size(width, measured.Height + 2);
            label.Font = font;
            label.ForeColor = color;
            label.BackColor = Color.Transparent;
            label.Text = text;
            parent.Controls.Add(label);
            return label;
        }

        private Guna2ToggleSwitch AddToggleRow(Guna2Panel page, ref int y, string text, string description)
        {
            Guna2ToggleSwitch toggle = new Guna2ToggleSwitch();
            toggle.Location = new Point(PageMarginX, y);
            toggle.Size = new Size(44, 22);
            toggle.BackColor = Background; // anahtarın yuvarlak gövdesi sayfa zeminine karışsın
            page.Controls.Add(toggle);

            Label label = MakeWrapLabel(page, text, new Font("Segoe UI", 9f), TextTone,
                PageMarginX + 58, y + 1, PageContentWidth - 58);

            y += Math.Max(22, label.Height) + 4;

            if (!string.IsNullOrEmpty(description))
            {
                Label desc = MakeWrapLabel(page, description, new Font("Segoe UI", 8.25f), MutedTone,
                    PageMarginX + 58, y, PageContentWidth - 58);
                y += desc.Height + 4;
            }

            y += 10;
            return toggle;
        }

        private Guna2TextBox AddTextRow(Guna2Panel page, ref int y, string labelText, int width, string placeholder)
        {
            Label label = MakeWrapLabel(page, labelText, new Font("Segoe UI", 9f), TextTone,
                PageMarginX, y, PageContentWidth);
            y += label.Height + 6;

            Guna2TextBox box = MakeTextBox(page, PageMarginX, y, width, placeholder);
            y = box.Bottom + 16;
            return box;
        }

        private Guna2TextBox AddMultilineRow(Guna2Panel page, ref int y, string labelText, int height)
        {
            Label label = MakeWrapLabel(page, labelText, new Font("Segoe UI", 9f), TextTone,
                PageMarginX, y, PageContentWidth);
            y += label.Height + 6;

            Guna2TextBox box = MakeTextBox(page, PageMarginX, y, PageContentWidth, string.Empty);
            box.Multiline = true;
            box.Size = new Size(PageContentWidth, height);
            box.ScrollBars = ScrollBars.Vertical;
            y = box.Bottom + 16;
            return box;
        }

        private Guna2TextBox MakeTextBox(Control parent, int x, int y, int width, string placeholder)
        {
            Guna2TextBox box = new Guna2TextBox();

            // Guna'nın kapsayıcı tabanlı kontrolleri (TextBox/NumericUpDown) yazı tipi
            // konum ve boyuttan SONRA verilirse kendilerini yeniden ölçekleyip hem büyüyor
            // hem aşağı kayıyor; altındaki satırların üzerine biniyordu. Bu yüzden önce
            // öz-ölçekleme kapatılır ve yazı tipi atanır, konum/boyut en sona bırakılır.
            box.AutoScaleMode = AutoScaleMode.None;
            box.Font = new Font("Segoe UI", 9.5f);
            box.BorderRadius = 8;
            box.FillColor = FieldFill;
            box.BackColor = Background; // yuvarlak köşeler sayfa zeminine karışsın
            box.BorderColor = BorderTone;
            box.ForeColor = TextTone;
            box.PlaceholderText = placeholder ?? string.Empty;
            box.PlaceholderForeColor = Color.FromArgb(110, 118, 132);

            // Pasifken Guna'nın açık gri varsayılanı koyu temayı bozuyor.
            box.DisabledState.FillColor = Color.FromArgb(24, 27, 33);
            box.DisabledState.BorderColor = Color.FromArgb(44, 49, 60);
            box.DisabledState.ForeColor = Color.FromArgb(118, 126, 140);
            box.DisabledState.PlaceholderForeColor = Color.FromArgb(84, 91, 104);

            parent.Controls.Add(box);
            box.Location = new Point(x, y);
            box.Size = new Size(width, 36);
            return box;
        }

        private Guna2NumericUpDown AddNumericRow(Guna2Panel page, ref int y, string labelText,
            decimal min, decimal max, decimal value)
        {
            Label label = MakeWrapLabel(page, labelText, new Font("Segoe UI", 9f), TextTone,
                PageMarginX, y, PageContentWidth);
            y += label.Height + 6;

            Guna2NumericUpDown numeric = MakeNumeric(page, PageMarginX, y, 150, min, max, value);
            y = numeric.Bottom + 16;
            return numeric;
        }

        /// <summary>Yan yana iki sayısal alan (ör. en az / en çok saniye).</summary>
        private void AddNumericPairRow(Guna2Panel page, ref int y,
            string label1, decimal min1, decimal max1, decimal value1, out Guna2NumericUpDown numeric1,
            string label2, decimal min2, decimal max2, decimal value2, out Guna2NumericUpDown numeric2)
        {
            const int columnWidth = 150;
            int x2 = PageMarginX + columnWidth + 28;

            Label a = MakeWrapLabel(page, label1, new Font("Segoe UI", 9f), TextTone, PageMarginX, y, columnWidth);
            Label b = MakeWrapLabel(page, label2, new Font("Segoe UI", 9f), TextTone, x2, y, columnWidth);
            y += Math.Max(a.Height, b.Height) + 6;

            numeric1 = MakeNumeric(page, PageMarginX, y, columnWidth, min1, max1, value1);
            numeric2 = MakeNumeric(page, x2, y, columnWidth, min2, max2, value2);
            y = Math.Max(numeric1.Bottom, numeric2.Bottom) + 16;
        }

        private Guna2NumericUpDown MakeNumeric(Control parent, int x, int y, int width,
            decimal min, decimal max, decimal value)
        {
            Guna2NumericUpDown numeric = new Guna2NumericUpDown();

            // MakeTextBox içindeki açıklamaya bakın: yazı tipi konumdan önce verilmezse
            // kontrol kendini yeniden ölçekleyip kayıyordu.
            numeric.AutoScaleMode = AutoScaleMode.None;
            numeric.Font = new Font("Segoe UI", 9.5f);
            numeric.BorderRadius = 8;
            numeric.FillColor = FieldFill;
            numeric.BackColor = Background; // yuvarlak köşeler sayfa zeminine karışsın
            numeric.BorderColor = BorderTone;
            numeric.ForeColor = TextTone;
            numeric.Minimum = min;
            numeric.Maximum = max;
            numeric.Value = value;
            parent.Controls.Add(numeric);
            numeric.Location = new Point(x, y);
            numeric.Size = new Size(width, 36);
            return numeric;
        }

        private Guna2ComboBox AddComboRow(Guna2Panel page, ref int y, string labelText, int width)
        {
            Label label = MakeWrapLabel(page, labelText, new Font("Segoe UI", 9f), TextTone,
                PageMarginX, y, PageContentWidth);
            y += label.Height + 6;

            Guna2ComboBox combo = new Guna2ComboBox();
            combo.Font = new Font("Segoe UI", 9.5f);
            combo.BorderRadius = 8;
            combo.FillColor = FieldFill;
            combo.BackColor = Background; // yuvarlak köşeler sayfa zeminine karışsın
            combo.BorderColor = BorderTone;
            combo.ForeColor = TextTone;
            combo.DropDownStyle = ComboBoxStyle.DropDownList;
            page.Controls.Add(combo);
            combo.Location = new Point(PageMarginX, y);
            combo.Size = new Size(width, 36);

            y = combo.Bottom + 16;
            return combo;
        }

        private void AddHelpRow(Guna2Panel page, ref int y, string text)
        {
            Label label = MakeWrapLabel(page, text, new Font("Segoe UI", 8.25f), MutedTone,
                PageMarginX, y, PageContentWidth);
            y += label.Height + 14;
        }

        private Guna2Button AddSmallButton(Guna2Panel page, string text, int x, int y, int width, Color fill)
        {
            Guna2Button button = new Guna2Button();
            button.Location = new Point(x, y);
            button.Size = new Size(width, 32);
            button.BorderRadius = 8;
            button.FillColor = fill;
            button.BackColor = Background; // yuvarlak köşeler sayfa zeminine karışsın
            button.ForeColor = Color.White;
            button.Font = new Font("Segoe UI", 8.75f, FontStyle.Bold);
            button.Cursor = Cursors.Hand;
            button.Text = text;
            button.HoverState.FillColor = ControlPaint.Light(fill, 0.15f);
            button.DisabledState.FillColor = Color.FromArgb(44, 48, 58);
            button.DisabledState.ForeColor = Color.FromArgb(96, 102, 116);
            page.Controls.Add(button);
            return button;
        }

        private Guna2DataGridView AddGridRow(Guna2Panel page, ref int y, int height)
        {
            Guna2DataGridView grid = new Guna2DataGridView();
            grid.Location = new Point(PageMarginX, y);
            grid.Size = new Size(PageContentWidth, height);
            grid.AllowUserToAddRows = false;
            grid.AllowUserToResizeRows = false;
            grid.RowHeadersVisible = false;
            grid.SelectionMode = DataGridViewSelectionMode.FullRowSelect;
            grid.EditMode = DataGridViewEditMode.EditOnEnter;
            grid.EnableHeadersVisualStyles = false;
            grid.BackgroundColor = FieldFill;
            grid.GridColor = BorderTone;
            grid.BorderStyle = BorderStyle.None;
            grid.ColumnHeadersHeightSizeMode = DataGridViewColumnHeadersHeightSizeMode.DisableResizing;
            grid.ColumnHeadersHeight = 30;
            grid.RowTemplate.Height = 28;
            grid.Font = new Font("Segoe UI", 8.5f);

            // Sütunlar ızgara genişliğini paylaşır: yatay kaydırma ve taşma hiç oluşmaz.
            grid.AutoSizeColumnsMode = DataGridViewAutoSizeColumnsMode.Fill;

            grid.ColumnHeadersDefaultCellStyle.BackColor = Color.FromArgb(24, 27, 33);
            grid.ColumnHeadersDefaultCellStyle.ForeColor = TextTone;
            grid.ColumnHeadersDefaultCellStyle.Font = new Font("Segoe UI", 8.25f, FontStyle.Bold);
            grid.ColumnHeadersDefaultCellStyle.SelectionBackColor = Color.FromArgb(24, 27, 33);
            grid.ColumnHeadersDefaultCellStyle.SelectionForeColor = TextTone;

            grid.DefaultCellStyle.BackColor = FieldFill;
            grid.DefaultCellStyle.ForeColor = TextTone;
            grid.DefaultCellStyle.SelectionBackColor = BorderTone;
            grid.DefaultCellStyle.SelectionForeColor = TextTone;

            grid.AlternatingRowsDefaultCellStyle.BackColor = Color.FromArgb(24, 27, 33);
            grid.AlternatingRowsDefaultCellStyle.ForeColor = TextTone;
            grid.AlternatingRowsDefaultCellStyle.SelectionBackColor = BorderTone;
            grid.AlternatingRowsDefaultCellStyle.SelectionForeColor = TextTone;

            page.Controls.Add(grid);
            y += height + 12;
            return grid;
        }

        private static void AddFillTextColumn(DataGridView grid, string name, string header,
            float fillWeight, int minWidth, string headerTip = null)
        {
            DataGridViewTextBoxColumn column = new DataGridViewTextBoxColumn();
            column.Name = name;
            column.HeaderText = header;
            column.AutoSizeMode = DataGridViewAutoSizeColumnMode.Fill;
            column.FillWeight = fillWeight;
            column.MinimumWidth = minWidth;
            column.HeaderCell.ToolTipText = headerTip ?? string.Empty;
            grid.Columns.Add(column);
        }

        private static void AddFixedTextColumn(DataGridView grid, string name, string header, int width,
            string headerTip = null)
        {
            DataGridViewTextBoxColumn column = new DataGridViewTextBoxColumn();
            column.Name = name;
            column.HeaderText = header;
            column.AutoSizeMode = DataGridViewAutoSizeColumnMode.None;
            column.Width = width;
            column.HeaderCell.ToolTipText = headerTip ?? string.Empty;
            grid.Columns.Add(column);
        }

        private static void AddCheckColumn(DataGridView grid, string name, string header, int width,
            string headerTip = null)
        {
            DataGridViewCheckBoxColumn column = new DataGridViewCheckBoxColumn();
            column.Name = name;
            column.HeaderText = header;
            column.AutoSizeMode = DataGridViewAutoSizeColumnMode.None;
            column.Width = width;
            column.HeaderCell.ToolTipText = headerTip ?? string.Empty;
            grid.Columns.Add(column);
        }

        // --- Sayfalar ---

        private void BuildPages()
        {
            BuildServerPage();
            BuildLoginCommandsPage();
            BuildSchedulerPage();
            BuildAntiAfkPage();
            BuildAutoRelogPage();
            BuildAutoEatPage();
            BuildAutoRespondPage();
            BuildChatLogPage();
            BuildAlertsPage();
        }

        private void BuildServerPage()
        {
            int y;
            Guna2Panel page = NewPage("Sunucu", "Sunucu ve Bağlantı",
                "Bu hesabın bağlanacağı sunucu ve başlatıcı açılışındaki davranışı.", out y);

            serverHostBox = AddTextRow(page, ref y,
                "Sunucu adresi (boş bırakılırsa varsayılan sunucu kullanılır)", 340, AfkDefaults.ServerHost);

            serverPortUpDown = AddNumericRow(page, ref y, "Port (0 = otomatik)", 0, 65535, 0);

            autoStartToggle = AddToggleRow(page, ref y,
                "Başlatıcı açılınca bu hesabı otomatik bağla", null);
        }

        private void BuildLoginCommandsPage()
        {
            int y;
            Guna2Panel page = NewPage("Giriş Komutları", "Giriş Komutları",
                "Oyuna girildiğinde sırayla yazılacak komut ya da sohbet mesajları.", out y);

            loginCommandsToggle = AddToggleRow(page, ref y, "Giriş komutlarını kullan", null);

            loginCommandsBox = AddMultilineRow(page, ref y, "Komutlar (her satıra bir tane)", 150);

            loginOnlyFirstToggle = AddToggleRow(page, ref y, "Yalnızca ilk girişte çalıştır",
                "Kapalıyken yeniden bağlanma dahil her girişte çalışır. Sunucu her girişte şifre istiyorsa kapalı bırakın.");

            loginDelayUpDown = AddNumericRow(page, ref y, "Bekleme (saniye): ilk komuttan önce ve komutlar arası", 0, 60, 2);

            AddHelpRow(page, ref y,
                "Her satır oyunda aynen yazılır: / ile başlayanlar komut, diğerleri sohbet mesajı olarak gönderilir. " +
                "Örnek: /kit ya da herkese merhaba");

            loginCommandsToggle.CheckedChanged += UpdateLoginCommandsDependents;
        }

        private void BuildSchedulerPage()
        {
            int y;
            Guna2Panel page = NewPage("Zamanlayıcı", "Zamanlayıcı (Görevler)",
                "Girişte ya da belirli aralıklarla kendiliğinden çalışan görevler.", out y);

            schedulerToggle = AddToggleRow(page, ref y, "Zamanlayıcıyı kullan", null);

            schedulerGrid = AddGridRow(page, ref y, 210);
            AddFillTextColumn(schedulerGrid, "Name", "Ad", 28f, 80);
            AddFillTextColumn(schedulerGrid, "Action", "Eylem", 44f, 120);
            AddCheckColumn(schedulerGrid, "OnLogin", "Girişte", 56);
            AddCheckColumn(schedulerGrid, "OnFirstLogin", "İlk girişte", 68);
            AddCheckColumn(schedulerGrid, "IntervalEnabled", "Aralık", 52);
            AddFixedTextColumn(schedulerGrid, "MinSeconds", "En az sn", 62);
            AddFixedTextColumn(schedulerGrid, "MaxSeconds", "En çok sn", 64);

            addTaskButton = AddSmallButton(page, "Görev Ekle", PageMarginX, y, 112, BlueTone);
            addTaskButton.Click += OnAddTaskRow;

            deleteTaskButton = AddSmallButton(page, "Seçiliyi Sil", PageMarginX + 120, y, 112, RedTone);
            deleteTaskButton.Click += OnDeleteTaskRow;
            y += 32 + 14;

            AddHelpRow(page, ref y,
                "Eylem sütunu istemci komutudur: \"send /kit\" sohbete /kit yazar, \"send merhaba\" mesaj gönderir. " +
                "Aralık açıksa görev, en az/en çok saniye arasından rastgele seçilen sürede bir yinelenir.");

            schedulerToggle.CheckedChanged += UpdateSchedulerDependents;
        }

        private void BuildAntiAfkPage()
        {
            int y;
            Guna2Panel page = NewPage("AFK Engelleme", "AFK Engelleme (AntiAFK)",
                "Sunucunun sizi AFK saymaması için düzenli aralıklarla hareket ve komut üretir.", out y);

            antiAfkToggle = AddToggleRow(page, ref y, "AFK engellemeyi kullan", null);

            AddNumericPairRow(page, ref y,
                "En az (saniye)", 1, 3600, 60, out antiAfkDelayMinUpDown,
                "En çok (saniye)", 1, 3600, 90, out antiAfkDelayMaxUpDown);

            antiAfkCommandBox = AddTextRow(page, ref y,
                "Her turda gönderilecek komut (boş bırakılabilir)", 260, "/ping");

            antiAfkSneakToggle = AddToggleRow(page, ref y, "Çömelerek hareket et", null);

            antiAfkTerrainToggle = AddToggleRow(page, ref y, "Rastgele yürü",
                "Arazi işlemesini otomatik açar; biraz daha fazla kaynak kullanır.");

            AddNumericPairRow(page, ref y,
                "Yürüme yarıçapı", 1, 32, 5, out antiAfkWalkRangeUpDown,
                "Deneme sayısı", 1, 100, 20, out antiAfkWalkRetriesUpDown);

            AddHelpRow(page, ref y,
                "Komut kutusu boş bırakılırsa sohbete hiçbir şey yazılmaz, yalnızca hareket edilir. " +
                "Sunucuda olmayan bir komut yazarsanız her turda hata mesajı alırsınız.");

            antiAfkToggle.CheckedChanged += UpdateAntiAfkDependents;
            antiAfkTerrainToggle.CheckedChanged += UpdateAntiAfkDependents;
        }

        private void BuildAutoRelogPage()
        {
            int y;
            Guna2Panel page = NewPage("Yeniden Bağlanma", "Yeniden Bağlanma (AutoRelog)",
                "Bağlantı koptuğunda ya da sunucudan atıldığınızda kendiliğinden yeniden girer.", out y);

            autoRelogToggle = AddToggleRow(page, ref y, "Yeniden bağlanmayı kullan", null);

            AddNumericPairRow(page, ref y,
                "En az (saniye)", 1, 3600, 5, out autoRelogDelayMinUpDown,
                "En çok (saniye)", 1, 3600, 10, out autoRelogDelayMaxUpDown);

            autoRelogUnlimitedToggle = AddToggleRow(page, ref y, "Sınırsız dene", null);

            autoRelogRetriesUpDown = AddNumericRow(page, ref y, "Deneme sayısı", 1, 100, 10);

            autoRelogIgnoreKickToggle = AddToggleRow(page, ref y, "Atılma mesajına bakma",
                "Açıkken sebep ne olursa olsun her kopuşta yeniden bağlanılır.");

            autoRelogKickMessagesBox = AddMultilineRow(page, ref y,
                "Yeniden bağlanmayı tetikleyen atılma mesajları (her satıra bir tane)", 96);

            autoRelogToggle.CheckedChanged += UpdateAutoRelogDependents;
            autoRelogUnlimitedToggle.CheckedChanged += UpdateAutoRelogDependents;
            autoRelogIgnoreKickToggle.CheckedChanged += UpdateAutoRelogDependents;
        }

        private void BuildAutoEatPage()
        {
            int y;
            Guna2Panel page = NewPage("Otomatik Yemek", "Otomatik Yemek (AutoEat)",
                "Açlık belirlediğiniz eşiğin altına inince envanterden yemek yer.", out y);

            autoEatToggle = AddToggleRow(page, ref y, "Otomatik yemeği kullan", null);

            autoEatThresholdUpDown = AddNumericRow(page, ref y, "Açlık eşiği (0-20)", 0, 20, 6);

            AddHelpRow(page, ref y,
                "Envanter işlemesini otomatik açar; bağlantı biraz daha fazla veri kullanır.");

            autoEatToggle.CheckedChanged += UpdateAutoEatDependents;
        }

        private void BuildAutoRespondPage()
        {
            int y;
            Guna2Panel page = NewPage("Otomatik Yanıt", "Otomatik Yanıt (AutoRespond)",
                "Sohbette sizin belirlediğiniz bir söz geçtiğinde hesap kendiliğinden yanıt verir.", out y);

            autoRespondToggle = AddToggleRow(page, ref y, "Otomatik yanıtı kullan", null);

            autoRespondSoundToggle = AddToggleRow(page, ref y, "Yanıt gönderilince bildirim sesi çal",
                "Hesap birine otomatik yanıt verdiğinde bilgisayarınızdan kısa bir uyarı sesi duyarsınız.");

            autoRespondMatchColorsToggle = AddToggleRow(page, ref y, "Renk kodlarıyla eşleştir",
                "Sunucu mesajlarındaki renk kodları da aranan söze dahil edilir. Emin değilseniz kapalı bırakın.");

            autoRespondGrid = AddGridRow(page, ref y, 170);
            AddFillTextColumn(autoRespondGrid, "Pattern", "Aranan söz", 30f, 70,
                "Sohbette bu söz geçtiğinde yanıt verilir. Örnek: selam");
            AddFillTextColumn(autoRespondGrid, "Action", "Yanıt", 30f, 70,
                "Oyunda söylenecek mesaj ya da /komut. $u yazarsanız yerine mesajı gönderen oyuncunun adı gelir.");
            AddFillTextColumn(autoRespondGrid, "PrivateAction", "Özel mesajda", 20f, 56,
                "Söz size özel mesajla (fısıltı) gelirse bunun yerine bu yanıt gönderilir. Boş bırakılabilir.");
            AddFillTextColumn(autoRespondGrid, "OtherAction", "Duyuruda", 20f, 56,
                "Söz bir oyuncudan değil sunucu duyurusundan gelirse bu yanıt gönderilir. Boş bırakılabilir.");
            AddCheckColumn(autoRespondGrid, "OwnersOnly", "Sadece ben", 60,
                "İşaretliyse yalnızca sizin ana hesabınızın yazdığı mesajlar yanıt tetikler.");
            AddFixedTextColumn(autoRespondGrid, "Cooldown", "Bekleme sn", 66,
                "Aynı kuralın iki yanıtı arasında geçmesi gereken süre (saniye). 0: beklemesiz.");
            AddCheckColumn(autoRespondGrid, "IsRegex", "Gelişmiş", 56,
                "Düzenli ifade (regex) bilenler için: işaretliyse aranan söz düz metin değil regex deseni sayılır.");

            addRespondRowButton = AddSmallButton(page, "Kural Ekle", PageMarginX, y, 112, BlueTone);
            addRespondRowButton.Click += OnAddRespondRow;

            deleteRespondRowButton = AddSmallButton(page, "Seçiliyi Sil", PageMarginX + 120, y, 112, RedTone);
            deleteRespondRowButton.Click += OnDeleteRespondRow;
            y += 32 + 14;

            AddHelpRow(page, ref y,
                "Nasıl çalışır: \"Aranan söz\" sohbette geçtiği anda \"Yanıt\" sütunundaki metin oyunda kendiliğinden " +
                "yazılır. Yanıt kutularına düz mesaj ya da /komut yazabilirsiniz; $u, mesajı gönderen oyuncunun adına " +
                "dönüşür. Örnek: aranan söz \"selam\", yanıt \"Merhaba $u, şu an klavye başında değilim!\". " +
                "Sütun başlıklarının üzerine gelerek ayrıntılı açıklamaları görebilirsiniz.");

            autoRespondToggle.CheckedChanged += UpdateAutoRespondDependents;
        }

        private void BuildChatLogPage()
        {
            int y;
            Guna2Panel page = NewPage("Sohbet Günlüğü", "Sohbet Günlüğü (ChatLog)",
                "Sohbeti hesabın kendi klasöründeki bir metin dosyasına kaydeder.", out y);

            chatLogToggle = AddToggleRow(page, ref y, "Sohbet günlüğünü kullan", null);

            chatLogDateTimeToggle = AddToggleRow(page, ref y, "Satırlara tarih ve saat ekle", null);

            chatLogFilterCombo = AddComboRow(page, ref y, "Kaydedilecek içerik", 220);
            chatLogFilterCombo.Items.Add("Tümü");
            chatLogFilterCombo.Items.Add("İletiler");
            chatLogFilterCombo.Items.Add("Sohbet");

            // Dosya kaydından bağımsız bir görünüm ayarı: canlı konsolu etkiler, bu yüzden
            // sohbet günlüğü kapalıyken de açılabilir (UpdateChatLogDependents'e dahil değildir).
            chatLogShowDateToggle = AddToggleRow(page, ref y, "Konsolda saatin yanında tarihi de göster",
                "Canlı konsol penceresindeki satırlara 11.07.2026 gibi tarih ekler. Varsayılan olarak yalnızca saat görünür.");

            chatLogToggle.CheckedChanged += UpdateChatLogDependents;
        }

        private void BuildAlertsPage()
        {
            int y;
            Guna2Panel page = NewPage("Uyarılar", "Uyarılar (Alerts)",
                "Sohbette aranan kelimeler geçtiğinde sesli ya da kayıtlı uyarı üretir.", out y);

            alertsToggle = AddToggleRow(page, ref y, "Uyarıları kullan", null);

            alertsBeepToggle = AddToggleRow(page, ref y, "Bip sesi çal", null);

            alertsTriggerWordsToggle = AddToggleRow(page, ref y, "Kelimelerde tetikle", null);

            alertsLogToFileToggle = AddToggleRow(page, ref y, "Dosyaya kaydet", null);

            alertsMatchesBox = AddMultilineRow(page, ref y, "Aranan kelimeler (her satıra bir tane)", 92);

            alertsExcludesBox = AddMultilineRow(page, ref y, "Hariç tutulan kelimeler", 92);

            alertsToggle.CheckedChanged += UpdateAlertsDependents;
        }

        // --- Satır ekle / sil ---

        private void OnAddRespondRow(object sender, EventArgs e)
        {
            autoRespondGrid.Rows.Add(string.Empty, string.Empty, string.Empty, string.Empty, false, "0", false);
        }

        private void OnDeleteRespondRow(object sender, EventArgs e)
        {
            DeleteSelectedRows(autoRespondGrid);
        }

        private void OnAddTaskRow(object sender, EventArgs e)
        {
            schedulerGrid.Rows.Add(string.Empty, string.Empty, false, false, true, "300", "600");
        }

        private void OnDeleteTaskRow(object sender, EventArgs e)
        {
            DeleteSelectedRows(schedulerGrid);
        }

        private static void DeleteSelectedRows(DataGridView grid)
        {
            List<DataGridViewRow> selected = new List<DataGridViewRow>();
            foreach (DataGridViewRow row in grid.SelectedRows)
                selected.Add(row);

            foreach (DataGridViewRow row in selected)
            {
                if (!row.IsNewRow)
                    grid.Rows.Remove(row);
            }
        }

        // --- Bağımlı kontrollerin etkin/pasif durumu ---

        private void UpdateLoginCommandsDependents(object sender, EventArgs e)
        {
            bool enabled = loginCommandsToggle.Checked;
            loginCommandsBox.Enabled = enabled;
            loginOnlyFirstToggle.Enabled = enabled;
            loginDelayUpDown.Enabled = enabled;
        }

        private void UpdateSchedulerDependents(object sender, EventArgs e)
        {
            bool enabled = schedulerToggle.Checked;
            schedulerGrid.Enabled = enabled;
            addTaskButton.Enabled = enabled;
            deleteTaskButton.Enabled = enabled;
        }

        private void UpdateAntiAfkDependents(object sender, EventArgs e)
        {
            bool enabled = antiAfkToggle.Checked;
            antiAfkDelayMinUpDown.Enabled = enabled;
            antiAfkDelayMaxUpDown.Enabled = enabled;
            antiAfkCommandBox.Enabled = enabled;
            antiAfkSneakToggle.Enabled = enabled;
            antiAfkTerrainToggle.Enabled = enabled;

            bool terrainEnabled = enabled && antiAfkTerrainToggle.Checked;
            antiAfkWalkRangeUpDown.Enabled = terrainEnabled;
            antiAfkWalkRetriesUpDown.Enabled = terrainEnabled;
        }

        private void UpdateAutoRelogDependents(object sender, EventArgs e)
        {
            bool enabled = autoRelogToggle.Checked;
            autoRelogDelayMinUpDown.Enabled = enabled;
            autoRelogDelayMaxUpDown.Enabled = enabled;
            autoRelogUnlimitedToggle.Enabled = enabled;
            autoRelogIgnoreKickToggle.Enabled = enabled;

            autoRelogRetriesUpDown.Enabled = enabled && !autoRelogUnlimitedToggle.Checked;
            autoRelogKickMessagesBox.Enabled = enabled && !autoRelogIgnoreKickToggle.Checked;
        }

        private void UpdateAutoEatDependents(object sender, EventArgs e)
        {
            autoEatThresholdUpDown.Enabled = autoEatToggle.Checked;
        }

        private void UpdateAutoRespondDependents(object sender, EventArgs e)
        {
            bool enabled = autoRespondToggle.Checked;
            autoRespondSoundToggle.Enabled = enabled;
            autoRespondMatchColorsToggle.Enabled = enabled;
            autoRespondGrid.Enabled = enabled;
            addRespondRowButton.Enabled = enabled;
            deleteRespondRowButton.Enabled = enabled;
        }

        private void UpdateChatLogDependents(object sender, EventArgs e)
        {
            bool enabled = chatLogToggle.Checked;
            chatLogDateTimeToggle.Enabled = enabled;
            chatLogFilterCombo.Enabled = enabled;
        }

        private void UpdateAlertsDependents(object sender, EventArgs e)
        {
            bool enabled = alertsToggle.Checked;
            alertsBeepToggle.Enabled = enabled;
            alertsTriggerWordsToggle.Enabled = enabled;
            alertsLogToFileToggle.Enabled = enabled;
            alertsMatchesBox.Enabled = enabled;
            alertsExcludesBox.Enabled = enabled;
        }

        // --- Hesaptan yükle ---

        private void LoadFromAccount()
        {
            serverHostBox.Text = account.ServerHost;
            serverPortUpDown.Value = Clamp(account.ServerPort, 0, 65535);
            autoStartToggle.Checked = account.AutoStart;

            LoginCommandsOptions login = account.LoginCommands ?? new LoginCommandsOptions();
            loginCommandsToggle.Checked = login.Enabled;
            loginCommandsBox.Lines = login.Commands.ToArray();
            loginOnlyFirstToggle.Checked = login.OnlyFirstLogin;
            loginDelayUpDown.Value = (decimal)ClampD(login.DelaySeconds, 0, 60);

            AntiAfkOptions antiAfk = account.AntiAfk;
            antiAfkToggle.Checked = antiAfk.Enabled;
            antiAfkDelayMinUpDown.Value = (decimal)ClampD(antiAfk.DelayMin, 1, 3600);
            antiAfkDelayMaxUpDown.Value = (decimal)ClampD(antiAfk.DelayMax, 1, 3600);
            antiAfkCommandBox.Text = antiAfk.Command;
            antiAfkSneakToggle.Checked = antiAfk.UseSneak;
            antiAfkTerrainToggle.Checked = antiAfk.UseTerrainHandling;
            antiAfkWalkRangeUpDown.Value = Clamp(antiAfk.WalkRange, 1, 32);
            antiAfkWalkRetriesUpDown.Value = Clamp(antiAfk.WalkRetries, 1, 100);

            AutoRelogOptions relog = account.AutoRelog;
            autoRelogToggle.Checked = relog.Enabled;
            autoRelogDelayMinUpDown.Value = (decimal)ClampD(relog.DelayMin, 1, 3600);
            autoRelogDelayMaxUpDown.Value = (decimal)ClampD(relog.DelayMax, 1, 3600);
            autoRelogUnlimitedToggle.Checked = relog.Retries < 0;
            autoRelogRetriesUpDown.Value = Clamp(relog.Retries < 0 ? 10 : relog.Retries, 1, 100);
            autoRelogIgnoreKickToggle.Checked = relog.IgnoreKickMessage;
            autoRelogKickMessagesBox.Lines = relog.KickMessages.ToArray();

            AutoEatOptions eat = account.AutoEat;
            autoEatToggle.Checked = eat.Enabled;
            autoEatThresholdUpDown.Value = Clamp(eat.Threshold, 0, 20);

            AutoRespondOptions respond = account.AutoRespond;
            autoRespondToggle.Checked = respond.Enabled;
            autoRespondSoundToggle.Checked = respond.SoundEnabled;
            autoRespondMatchColorsToggle.Checked = respond.MatchColors;
            foreach (AutoRespondMatch match in respond.Matches)
            {
                autoRespondGrid.Rows.Add(
                    match.Pattern,
                    ActionToDisplay(match.Action),
                    ActionToDisplay(match.PrivateAction),
                    ActionToDisplay(match.OtherAction),
                    match.OwnersOnly,
                    match.CooldownSeconds.ToString(CultureInfo.InvariantCulture),
                    match.IsRegex);
            }

            ChatLogOptions chatLog = account.ChatLog;
            chatLogToggle.Checked = chatLog.Enabled;
            chatLogDateTimeToggle.Checked = chatLog.AddDateTime;
            chatLogShowDateToggle.Checked = chatLog.ShowDate;
            chatLogFilterCombo.SelectedIndex = FilterToIndex(chatLog.Filter);

            AlertsOptions alerts = account.Alerts;
            alertsToggle.Checked = alerts.Enabled;
            alertsBeepToggle.Checked = alerts.BeepEnabled;
            alertsTriggerWordsToggle.Checked = alerts.TriggerByWords;
            alertsLogToFileToggle.Checked = alerts.LogToFile;
            alertsMatchesBox.Lines = alerts.Matches.ToArray();
            alertsExcludesBox.Lines = alerts.Excludes.ToArray();

            SchedulerOptions scheduler = account.Scheduler;
            schedulerToggle.Checked = scheduler.Enabled;
            foreach (ScheduledTask task in scheduler.Tasks)
            {
                schedulerGrid.Rows.Add(
                    task.Name,
                    task.Action,
                    task.OnLogin,
                    task.OnFirstLogin,
                    task.IntervalEnabled,
                    task.MinSeconds.ToString(CultureInfo.InvariantCulture),
                    task.MaxSeconds.ToString(CultureInfo.InvariantCulture));
            }

            // Bağımlı kontrollerin ilk etkin/pasif durumu; sonrasında ilgili CheckedChanged olayları devralır.
            UpdateLoginCommandsDependents(null, EventArgs.Empty);
            UpdateSchedulerDependents(null, EventArgs.Empty);
            UpdateAntiAfkDependents(null, EventArgs.Empty);
            UpdateAutoRelogDependents(null, EventArgs.Empty);
            UpdateAutoEatDependents(null, EventArgs.Empty);
            UpdateAutoRespondDependents(null, EventArgs.Empty);
            UpdateChatLogDependents(null, EventArgs.Empty);
            UpdateAlertsDependents(null, EventArgs.Empty);
        }

        // --- Kaydet ---

        private void OnSaveClicked(object sender, EventArgs e)
        {
            // Izgaralarda düzenleme hâlindeki son hücreyi kesinleştir: kullanıcı bir hücreyi
            // (özellikle onay kutusunu) değiştirip doğrudan Kaydet'e basarsa, değer okunmadan
            // önce hücreye işlensin.
            autoRespondGrid.EndEdit();
            schedulerGrid.EndEdit();

            // Komut boş bırakılabilir: bu durumda AFK engelleme yalnızca hareket eder. Sunucuda karşılığı
            // olmayan bir komut yazmak, her turda "bilinmeyen komut" yanıtı almaktan başka işe yaramaz.
            string antiAfkCommand = antiAfkCommandBox.Text.Trim();

            List<string> loginCommands = SplitLines(loginCommandsBox.Lines);
            if (loginCommandsToggle.Checked && loginCommands.Count == 0)
            {
                SelectPage(1);
                MessageBox.Show(this, "Giriş komutları açık ama komut listesi boş.",
                    "Eksik Bilgi", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }

            List<AutoRespondMatch> respondMatches = CollectRespondMatches();
            if (autoRespondToggle.Checked && respondMatches.Count == 0)
            {
                SelectPage(6);
                MessageBox.Show(this, "Otomatik Yanıt açık ama en az bir geçerli eşleşme yok.",
                    "Eksik Bilgi", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }

            List<ScheduledTask> tasks = CollectScheduledTasks();
            if (schedulerToggle.Checked && tasks.Count == 0)
            {
                SelectPage(2);
                MessageBox.Show(this, "Zamanlayıcı açık ama görev yok.",
                    "Eksik Bilgi", MessageBoxButtons.OK, MessageBoxIcon.Warning);
                return;
            }

            ApplyToAccount(antiAfkCommand, loginCommands, respondMatches, tasks);

            DialogResult = DialogResult.OK;
            Close();
        }

        private void ApplyToAccount(string antiAfkCommand, List<string> loginCommands,
            List<AutoRespondMatch> respondMatches, List<ScheduledTask> tasks)
        {
            account.ServerHost = serverHostBox.Text.Trim();
            account.ServerPort = (int)serverPortUpDown.Value;
            account.AutoStart = autoStartToggle.Checked;

            if (account.LoginCommands == null)
                account.LoginCommands = new LoginCommandsOptions();

            LoginCommandsOptions login = account.LoginCommands;
            login.Enabled = loginCommandsToggle.Checked;
            login.OnlyFirstLogin = loginOnlyFirstToggle.Checked;
            login.DelaySeconds = (double)loginDelayUpDown.Value;
            login.Commands = loginCommands;

            double antiAfkMin = (double)antiAfkDelayMinUpDown.Value;
            double antiAfkMax = (double)antiAfkDelayMaxUpDown.Value;
            NormalizeRange(ref antiAfkMin, ref antiAfkMax);

            AntiAfkOptions antiAfk = account.AntiAfk;
            antiAfk.Enabled = antiAfkToggle.Checked;
            antiAfk.DelayMin = antiAfkMin;
            antiAfk.DelayMax = antiAfkMax;
            antiAfk.Command = antiAfkCommand;
            antiAfk.UseSneak = antiAfkSneakToggle.Checked;
            antiAfk.UseTerrainHandling = antiAfkTerrainToggle.Checked;
            antiAfk.WalkRange = (int)antiAfkWalkRangeUpDown.Value;
            antiAfk.WalkRetries = (int)antiAfkWalkRetriesUpDown.Value;

            double relogMin = (double)autoRelogDelayMinUpDown.Value;
            double relogMax = (double)autoRelogDelayMaxUpDown.Value;
            NormalizeRange(ref relogMin, ref relogMax);

            AutoRelogOptions relog = account.AutoRelog;
            relog.Enabled = autoRelogToggle.Checked;
            relog.DelayMin = relogMin;
            relog.DelayMax = relogMax;
            relog.Retries = autoRelogUnlimitedToggle.Checked ? -1 : (int)autoRelogRetriesUpDown.Value;
            relog.IgnoreKickMessage = autoRelogIgnoreKickToggle.Checked;
            relog.KickMessages = SplitLines(autoRelogKickMessagesBox.Lines);

            AutoEatOptions eat = account.AutoEat;
            eat.Enabled = autoEatToggle.Checked;
            eat.Threshold = (int)autoEatThresholdUpDown.Value;

            AutoRespondOptions respond = account.AutoRespond;
            respond.Enabled = autoRespondToggle.Checked;
            respond.SoundEnabled = autoRespondSoundToggle.Checked;
            respond.MatchColors = autoRespondMatchColorsToggle.Checked;
            respond.Matches = respondMatches;

            ChatLogOptions chatLog = account.ChatLog;
            chatLog.Enabled = chatLogToggle.Checked;
            chatLog.AddDateTime = chatLogDateTimeToggle.Checked;
            chatLog.ShowDate = chatLogShowDateToggle.Checked;
            chatLog.Filter = IndexToFilter(chatLogFilterCombo.SelectedIndex);

            AlertsOptions alerts = account.Alerts;
            alerts.Enabled = alertsToggle.Checked;
            alerts.BeepEnabled = alertsBeepToggle.Checked;
            alerts.TriggerByWords = alertsTriggerWordsToggle.Checked;
            alerts.LogToFile = alertsLogToFileToggle.Checked;
            alerts.Matches = SplitLines(alertsMatchesBox.Lines);
            alerts.Excludes = SplitLines(alertsExcludesBox.Lines);

            SchedulerOptions scheduler = account.Scheduler;
            scheduler.Enabled = schedulerToggle.Checked;
            scheduler.Tasks = tasks;
        }

        private List<AutoRespondMatch> CollectRespondMatches()
        {
            List<AutoRespondMatch> list = new List<AutoRespondMatch>();

            foreach (DataGridViewRow row in autoRespondGrid.Rows)
            {
                if (row.IsNewRow)
                    continue;

                string pattern = CellText(row, 0);
                string action = DisplayToAction(CellText(row, 1));
                string privateAction = DisplayToAction(CellText(row, 2));
                string otherAction = DisplayToAction(CellText(row, 3));

                if (string.IsNullOrEmpty(pattern))
                    continue;
                if (string.IsNullOrEmpty(action) && string.IsNullOrEmpty(privateAction) && string.IsNullOrEmpty(otherAction))
                    continue;

                AutoRespondMatch match = new AutoRespondMatch();
                match.Pattern = pattern;
                match.IsRegex = CellBool(row, 6);
                match.Action = action;
                match.PrivateAction = privateAction;
                match.OtherAction = otherAction;
                match.OwnersOnly = CellBool(row, 4);

                int cooldown;
                if (!int.TryParse(CellText(row, 5), NumberStyles.Integer, CultureInfo.InvariantCulture, out cooldown))
                    cooldown = 0;
                match.CooldownSeconds = cooldown;

                list.Add(match);
            }

            return list;
        }

        // --- Yanıt metni <-> MCC eylemi çevrimi ---

        // Kullanıcı yanıt kutusuna oyunda söylenecek metni ya da /komutu yazar; MCC ise
        // "send <metin>" biçiminde bir eylem bekler. Bu küme, "send" dışındaki MCC eylem
        // fiilleriyle (log, script...) başlayan eski/gelişmiş kayıtların bozulmadan geçmesini sağlar.
        private static readonly HashSet<string> RawActionVerbs = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            "log", "script", "connect", "disconnect", "respawn", "move", "useitem", "dig", "sneak", "wait"
        };

        /// <summary>Ham MCC eylemini kullanıcıya gösterilecek sade metne çevirir ("send selam" -> "selam").</summary>
        private static string ActionToDisplay(string raw)
        {
            if (string.IsNullOrEmpty(raw))
                return string.Empty;

            return raw.StartsWith("send ", StringComparison.OrdinalIgnoreCase)
                ? raw.Substring(5)
                : raw;
        }

        /// <summary>Kullanıcının yazdığı yanıtı MCC eylemine çevirir ("selam" -> "send selam").</summary>
        private static string DisplayToAction(string display)
        {
            string text = display == null ? string.Empty : display.Trim();
            if (text.Length == 0)
                return string.Empty;

            int space = text.IndexOf(' ');
            string firstWord = space < 0 ? text : text.Substring(0, space);

            return RawActionVerbs.Contains(firstWord) ? text : "send " + text;
        }

        private List<ScheduledTask> CollectScheduledTasks()
        {
            List<ScheduledTask> list = new List<ScheduledTask>();

            foreach (DataGridViewRow row in schedulerGrid.Rows)
            {
                if (row.IsNewRow)
                    continue;

                string action = CellText(row, 1);
                if (string.IsNullOrEmpty(action))
                    continue;

                string name = CellText(row, 0);
                if (string.IsNullOrEmpty(name))
                    name = "Görev";

                double minSeconds;
                if (!double.TryParse(CellText(row, 5), NumberStyles.Float, CultureInfo.InvariantCulture, out minSeconds))
                    minSeconds = 300;

                double maxSeconds;
                if (!double.TryParse(CellText(row, 6), NumberStyles.Float, CultureInfo.InvariantCulture, out maxSeconds))
                    maxSeconds = 600;

                NormalizeRange(ref minSeconds, ref maxSeconds);

                ScheduledTask task = new ScheduledTask();
                task.Name = name;
                task.Action = action;
                task.OnLogin = CellBool(row, 2);
                task.OnFirstLogin = CellBool(row, 3);
                task.IntervalEnabled = CellBool(row, 4);
                task.MinSeconds = minSeconds;
                task.MaxSeconds = maxSeconds;

                list.Add(task);
            }

            return list;
        }

        // --- Küçük yardımcılar ---

        private static int Clamp(int value, int min, int max)
        {
            if (value < min) return min;
            if (value > max) return max;
            return value;
        }

        private static double ClampD(double value, double min, double max)
        {
            if (value < min) return min;
            if (value > max) return max;
            return value;
        }

        private static void NormalizeRange(ref double min, ref double max)
        {
            if (min > max)
            {
                double temp = min;
                min = max;
                max = temp;
            }
        }

        private static List<string> SplitLines(string[] lines)
        {
            List<string> list = new List<string>();
            foreach (string line in lines)
            {
                string trimmed = line.Trim();
                if (!string.IsNullOrEmpty(trimmed))
                    list.Add(trimmed);
            }
            return list;
        }

        private static bool CellBool(DataGridViewRow row, int columnIndex)
        {
            object value = row.Cells[columnIndex].Value;
            return value != null && Convert.ToBoolean(value);
        }

        private static string CellText(DataGridViewRow row, int columnIndex)
        {
            object value = row.Cells[columnIndex].Value;
            return value == null ? string.Empty : Convert.ToString(value).Trim();
        }

        private static int FilterToIndex(string filter)
        {
            if (string.Equals(filter, "all", StringComparison.OrdinalIgnoreCase))
                return 0;
            if (string.Equals(filter, "chat", StringComparison.OrdinalIgnoreCase))
                return 2;
            return 1; // "messages" varsayılan
        }

        private static string IndexToFilter(int index)
        {
            switch (index)
            {
                case 0: return "all";
                case 2: return "chat";
                default: return "messages";
            }
        }

        protected override void Dispose(bool disposing)
        {
            if (disposing)
            {
                if (formElipse != null)
                    formElipse.Dispose();
                if (dragControl != null)
                    dragControl.Dispose();
            }

            base.Dispose(disposing);
        }
    }
}
