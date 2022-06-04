
namespace Projects_Launcher
{
    partial class loginMenuForm
    {
        /// <summary>
        ///Gerekli tasarımcı değişkeni.
        /// </summary>
        private System.ComponentModel.IContainer components = null;

        /// <summary>
        ///Kullanılan tüm kaynakları temizleyin.
        /// </summary>
        ///<param name="disposing">yönetilen kaynaklar dispose edilmeliyse doğru; aksi halde yanlış.</param>
        protected override void Dispose(bool disposing)
        {
            if (disposing && (components != null))
            {
                components.Dispose();
            }
            base.Dispose(disposing);
        }

        #region Windows Form Designer üretilen kod

        /// <summary>
        /// Tasarımcı desteği için gerekli metot - bu metodun 
        ///içeriğini kod düzenleyici ile değiştirmeyin.
        /// </summary>
        private void InitializeComponent()
        {
            this.components = new System.ComponentModel.Container();
            System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(loginMenuForm));
            this.guna2BorderlessForm1 = new Guna.UI2.WinForms.Guna2BorderlessForm(this.components);
            this.raminfo = new System.Windows.Forms.Label();
            this.rememberMeCheckBox = new Guna.UI2.WinForms.Guna2CheckBox();
            this.nickNameEnterTextBox = new Guna.UI2.WinForms.Guna2TextBox();
            this.closeButtonControlBox = new Guna.UI2.WinForms.Guna2ControlBox();
            this.minimizeButtonControlBox = new Guna.UI2.WinForms.Guna2ControlBox();
            this.guna2BorderlessForm2 = new Guna.UI2.WinForms.Guna2BorderlessForm(this.components);
            this.maximizeButtonControlBox = new Guna.UI2.WinForms.Guna2ControlBox();
            this.updateNowButton = new System.Windows.Forms.Button();
            this.backButton = new System.Windows.Forms.Button();
            this.newsLabel = new Guna.UI2.WinForms.Guna2ImageButton();
            this.webbutton = new Guna.UI2.WinForms.Guna2ImageButton();
            this.loginButton = new Guna.UI2.WinForms.Guna2ImageButton();
            this.newVersionPanel = new System.Windows.Forms.Panel();
            this.vCurrentLabel = new System.Windows.Forms.Label();
            this.vLatestLabel = new System.Windows.Forms.Label();
            this.skipButton = new System.Windows.Forms.Button();
            this.noUpdateButton = new System.Windows.Forms.Button();
            this.updateButton = new System.Windows.Forms.Button();
            this.vLatestLabelT = new System.Windows.Forms.Label();
            this.vCurrentLabelT = new System.Windows.Forms.Label();
            this.updateLabelBottom = new System.Windows.Forms.Label();
            this.updateLabelTop = new System.Windows.Forms.Label();
            this.updateHeadline = new System.Windows.Forms.Label();
            this.panelYenilikler = new Guna.UI2.WinForms.Guna2Panel();
            this.versionLabel = new System.Windows.Forms.Label();
            this.guna2PictureBox2 = new Guna.UI2.WinForms.Guna2PictureBox();
            this.labelYenilikMaddeler = new System.Windows.Forms.Label();
            this.newVersionPanel.SuspendLayout();
            this.panelYenilikler.SuspendLayout();
            ((System.ComponentModel.ISupportInitialize)(this.guna2PictureBox2)).BeginInit();
            this.SuspendLayout();
            // 
            // guna2BorderlessForm1
            // 
            this.guna2BorderlessForm1.ContainerControl = this;
            this.guna2BorderlessForm1.DockIndicatorTransparencyValue = 0.6D;
            this.guna2BorderlessForm1.TransparentWhileDrag = true;
            // 
            // raminfo
            // 
            this.raminfo.AutoSize = true;
            this.raminfo.BackColor = System.Drawing.Color.Transparent;
            this.raminfo.Cursor = System.Windows.Forms.Cursors.Hand;
            this.raminfo.Font = new System.Drawing.Font("Arial", 15.75F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.raminfo.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.raminfo.Location = new System.Drawing.Point(452, 834);
            this.raminfo.Name = "raminfo";
            this.raminfo.Size = new System.Drawing.Size(58, 24);
            this.raminfo.TabIndex = 33;
            this.raminfo.Text = "4096";
            // 
            // rememberMeCheckBox
            // 
            this.rememberMeCheckBox.Anchor = System.Windows.Forms.AnchorStyles.None;
            this.rememberMeCheckBox.Animated = true;
            this.rememberMeCheckBox.AutoCheck = false;
            this.rememberMeCheckBox.AutoSize = true;
            this.rememberMeCheckBox.BackColor = System.Drawing.Color.Transparent;
            this.rememberMeCheckBox.Checked = true;
            this.rememberMeCheckBox.CheckedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(248)))), ((int)(((byte)(148)))), ((int)(((byte)(35)))));
            this.rememberMeCheckBox.CheckedState.BorderRadius = 0;
            this.rememberMeCheckBox.CheckedState.BorderThickness = 0;
            this.rememberMeCheckBox.CheckedState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(248)))), ((int)(((byte)(148)))), ((int)(((byte)(35)))));
            this.rememberMeCheckBox.CheckState = System.Windows.Forms.CheckState.Checked;
            this.rememberMeCheckBox.Font = new System.Drawing.Font("Microsoft Sans Serif", 12F);
            this.rememberMeCheckBox.ForeColor = System.Drawing.SystemColors.ControlLightLight;
            this.rememberMeCheckBox.Location = new System.Drawing.Point(380, 205);
            this.rememberMeCheckBox.Name = "rememberMeCheckBox";
            this.rememberMeCheckBox.Size = new System.Drawing.Size(110, 24);
            this.rememberMeCheckBox.TabIndex = 32;
            this.rememberMeCheckBox.Text = "Beni Hatırla";
            this.rememberMeCheckBox.UncheckedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(125)))), ((int)(((byte)(137)))), ((int)(((byte)(149)))));
            this.rememberMeCheckBox.UncheckedState.BorderRadius = 0;
            this.rememberMeCheckBox.UncheckedState.BorderThickness = 0;
            this.rememberMeCheckBox.UncheckedState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(125)))), ((int)(((byte)(137)))), ((int)(((byte)(149)))));
            this.rememberMeCheckBox.UseVisualStyleBackColor = false;
            this.rememberMeCheckBox.CheckedChanged += new System.EventHandler(this.benihatırla_CheckedChanged);
            // 
            // nickNameEnterTextBox
            // 
            this.nickNameEnterTextBox.Anchor = System.Windows.Forms.AnchorStyles.None;
            this.nickNameEnterTextBox.BackColor = System.Drawing.Color.Tomato;
            this.nickNameEnterTextBox.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(64)))), ((int)(((byte)(64)))), ((int)(((byte)(64)))));
            this.nickNameEnterTextBox.Cursor = System.Windows.Forms.Cursors.IBeam;
            this.nickNameEnterTextBox.DefaultText = "Kullanıcı Adı";
            this.nickNameEnterTextBox.DisabledState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(208)))), ((int)(((byte)(208)))), ((int)(((byte)(208)))));
            this.nickNameEnterTextBox.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(226)))), ((int)(((byte)(226)))), ((int)(((byte)(226)))));
            this.nickNameEnterTextBox.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.nickNameEnterTextBox.DisabledState.PlaceholderForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.nickNameEnterTextBox.FillColor = System.Drawing.Color.Transparent;
            this.nickNameEnterTextBox.FocusedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.nickNameEnterTextBox.Font = new System.Drawing.Font("Segoe UI", 14.25F);
            this.nickNameEnterTextBox.ForeColor = System.Drawing.Color.White;
            this.nickNameEnterTextBox.HoverState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.nickNameEnterTextBox.IconLeft = global::Projects_Launcher.Properties.Resources.projects_Logo_64x64;
            this.nickNameEnterTextBox.Location = new System.Drawing.Point(380, 171);
            this.nickNameEnterTextBox.Margin = new System.Windows.Forms.Padding(4);
            this.nickNameEnterTextBox.Name = "nickNameEnterTextBox";
            this.nickNameEnterTextBox.PasswordChar = '\0';
            this.nickNameEnterTextBox.PlaceholderForeColor = System.Drawing.Color.Black;
            this.nickNameEnterTextBox.PlaceholderText = "";
            this.nickNameEnterTextBox.SelectedText = "";
            this.nickNameEnterTextBox.Size = new System.Drawing.Size(220, 34);
            this.nickNameEnterTextBox.TabIndex = 26;
            this.nickNameEnterTextBox.TextChanged += new System.EventHandler(this.nicknametextbox_TextChanged);
            // 
            // closeButtonControlBox
            // 
            this.closeButtonControlBox.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.closeButtonControlBox.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(25)))), ((int)(((byte)(25)))));
            this.closeButtonControlBox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(25)))), ((int)(((byte)(25)))));
            this.closeButtonControlBox.IconColor = System.Drawing.Color.White;
            this.closeButtonControlBox.Location = new System.Drawing.Point(935, 0);
            this.closeButtonControlBox.Name = "closeButtonControlBox";
            this.closeButtonControlBox.Size = new System.Drawing.Size(45, 29);
            this.closeButtonControlBox.TabIndex = 25;
            this.closeButtonControlBox.Click += new System.EventHandler(this.guna2ControlBox1_Click);
            // 
            // minimizeButtonControlBox
            // 
            this.minimizeButtonControlBox.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.minimizeButtonControlBox.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(25)))), ((int)(((byte)(25)))));
            this.minimizeButtonControlBox.ControlBoxType = Guna.UI2.WinForms.Enums.ControlBoxType.MinimizeBox;
            this.minimizeButtonControlBox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(25)))), ((int)(((byte)(25)))));
            this.minimizeButtonControlBox.IconColor = System.Drawing.Color.White;
            this.minimizeButtonControlBox.Location = new System.Drawing.Point(849, 0);
            this.minimizeButtonControlBox.Name = "minimizeButtonControlBox";
            this.minimizeButtonControlBox.Size = new System.Drawing.Size(45, 29);
            this.minimizeButtonControlBox.TabIndex = 136;
            this.minimizeButtonControlBox.Click += new System.EventHandler(this.guna2ControlBox2_Click);
            // 
            // guna2BorderlessForm2
            // 
            this.guna2BorderlessForm2.ContainerControl = this;
            this.guna2BorderlessForm2.DockIndicatorTransparencyValue = 0.6D;
            this.guna2BorderlessForm2.TransparentWhileDrag = true;
            // 
            // maximizeButtonControlBox
            // 
            this.maximizeButtonControlBox.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.maximizeButtonControlBox.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(25)))), ((int)(((byte)(25)))));
            this.maximizeButtonControlBox.ControlBoxType = Guna.UI2.WinForms.Enums.ControlBoxType.MaximizeBox;
            this.maximizeButtonControlBox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(25)))), ((int)(((byte)(25)))));
            this.maximizeButtonControlBox.IconColor = System.Drawing.Color.White;
            this.maximizeButtonControlBox.Location = new System.Drawing.Point(893, 0);
            this.maximizeButtonControlBox.Name = "maximizeButtonControlBox";
            this.maximizeButtonControlBox.Size = new System.Drawing.Size(45, 29);
            this.maximizeButtonControlBox.TabIndex = 139;
            // 
            // updateNowButton
            // 
            this.updateNowButton.FlatStyle = System.Windows.Forms.FlatStyle.Flat;
            this.updateNowButton.Font = new System.Drawing.Font("Arial", 9F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.updateNowButton.ForeColor = System.Drawing.SystemColors.ButtonHighlight;
            this.updateNowButton.Location = new System.Drawing.Point(849, 468);
            this.updateNowButton.Name = "updateNowButton";
            this.updateNowButton.Size = new System.Drawing.Size(115, 23);
            this.updateNowButton.TabIndex = 141;
            this.updateNowButton.Text = "Şimdi Güncelle";
            this.updateNowButton.UseVisualStyleBackColor = true;
            this.updateNowButton.Visible = false;
            this.updateNowButton.Click += new System.EventHandler(this.updateNowButton_Click);
            // 
            // backButton
            // 
            this.backButton.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(25)))), ((int)(((byte)(25)))));
            this.backButton.FlatStyle = System.Windows.Forms.FlatStyle.Popup;
            this.backButton.ForeColor = System.Drawing.SystemColors.ButtonHighlight;
            this.backButton.Location = new System.Drawing.Point(0, 0);
            this.backButton.Name = "backButton";
            this.backButton.Size = new System.Drawing.Size(45, 29);
            this.backButton.TabIndex = 200;
            this.backButton.Text = "←";
            this.backButton.UseVisualStyleBackColor = false;
            this.backButton.Visible = false;
            this.backButton.Click += new System.EventHandler(this.backButton_Click);
            // 
            // newsLabel
            // 
            this.newsLabel.CheckedState.ImageSize = new System.Drawing.Size(64, 64);
            this.newsLabel.HoverState.Image = ((System.Drawing.Image)(resources.GetObject("resource.Image2")));
            this.newsLabel.HoverState.ImageSize = new System.Drawing.Size(191, 25);
            this.newsLabel.Image = ((System.Drawing.Image)(resources.GetObject("newsLabel.Image")));
            this.newsLabel.ImageOffset = new System.Drawing.Point(0, 0);
            this.newsLabel.ImageRotate = 0F;
            this.newsLabel.ImageSize = new System.Drawing.Size(191, 25);
            this.newsLabel.Location = new System.Drawing.Point(389, 457);
            this.newsLabel.Name = "newsLabel";
            this.newsLabel.PressedState.ImageSize = new System.Drawing.Size(64, 64);
            this.newsLabel.Size = new System.Drawing.Size(207, 25);
            this.newsLabel.TabIndex = 1023;
            this.newsLabel.Click += new System.EventHandler(this.newsLabel_Click_1);
            // 
            // webbutton
            // 
            this.webbutton.CheckedState.ImageSize = new System.Drawing.Size(64, 64);
            this.webbutton.HoverState.Image = ((System.Drawing.Image)(resources.GetObject("resource.Image1")));
            this.webbutton.HoverState.ImageSize = new System.Drawing.Size(207, 21);
            this.webbutton.Image = ((System.Drawing.Image)(resources.GetObject("webbutton.Image")));
            this.webbutton.ImageOffset = new System.Drawing.Point(0, 0);
            this.webbutton.ImageRotate = 0F;
            this.webbutton.ImageSize = new System.Drawing.Size(207, 21);
            this.webbutton.Location = new System.Drawing.Point(389, 325);
            this.webbutton.Name = "webbutton";
            this.webbutton.PressedState.ImageSize = new System.Drawing.Size(64, 64);
            this.webbutton.Size = new System.Drawing.Size(207, 21);
            this.webbutton.TabIndex = 1024;
            this.webbutton.Click += new System.EventHandler(this.webbutton_Click);
            // 
            // loginButton
            // 
            this.loginButton.CheckedState.ImageSize = new System.Drawing.Size(64, 64);
            this.loginButton.HoverState.Image = ((System.Drawing.Image)(resources.GetObject("resource.Image")));
            this.loginButton.HoverState.ImageSize = new System.Drawing.Size(207, 59);
            this.loginButton.Image = global::Projects_Launcher.Properties.Resources.giris;
            this.loginButton.ImageOffset = new System.Drawing.Point(0, 0);
            this.loginButton.ImageRotate = 0F;
            this.loginButton.ImageSize = new System.Drawing.Size(207, 59);
            this.loginButton.Location = new System.Drawing.Point(389, 260);
            this.loginButton.Name = "loginButton";
            this.loginButton.PressedState.ImageSize = new System.Drawing.Size(64, 64);
            this.loginButton.Size = new System.Drawing.Size(207, 59);
            this.loginButton.TabIndex = 1025;
            this.loginButton.Click += new System.EventHandler(this.girisyapbutton_Click);
            // 
            // newVersionPanel
            // 
            this.newVersionPanel.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(40)))), ((int)(((byte)(40)))), ((int)(((byte)(40)))));
            this.newVersionPanel.Controls.Add(this.vCurrentLabel);
            this.newVersionPanel.Controls.Add(this.vLatestLabel);
            this.newVersionPanel.Controls.Add(this.skipButton);
            this.newVersionPanel.Controls.Add(this.noUpdateButton);
            this.newVersionPanel.Controls.Add(this.updateButton);
            this.newVersionPanel.Controls.Add(this.vLatestLabelT);
            this.newVersionPanel.Controls.Add(this.vCurrentLabelT);
            this.newVersionPanel.Controls.Add(this.updateLabelBottom);
            this.newVersionPanel.Controls.Add(this.updateLabelTop);
            this.newVersionPanel.Controls.Add(this.updateHeadline);
            this.newVersionPanel.Location = new System.Drawing.Point(163, 113);
            this.newVersionPanel.Name = "newVersionPanel";
            this.newVersionPanel.Size = new System.Drawing.Size(633, 288);
            this.newVersionPanel.TabIndex = 1026;
            this.newVersionPanel.Visible = false;
            // 
            // vCurrentLabel
            // 
            this.vCurrentLabel.AutoSize = true;
            this.vCurrentLabel.BackColor = System.Drawing.Color.Transparent;
            this.vCurrentLabel.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.vCurrentLabel.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(200)))), ((int)(((byte)(45)))), ((int)(((byte)(35)))));
            this.vCurrentLabel.Location = new System.Drawing.Point(144, 106);
            this.vCurrentLabel.Name = "vCurrentLabel";
            this.vCurrentLabel.Size = new System.Drawing.Size(37, 18);
            this.vCurrentLabel.TabIndex = 9;
            this.vCurrentLabel.Text = "x.x.x";
            // 
            // vLatestLabel
            // 
            this.vLatestLabel.AutoSize = true;
            this.vLatestLabel.BackColor = System.Drawing.Color.Transparent;
            this.vLatestLabel.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.vLatestLabel.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(50)))), ((int)(((byte)(200)))), ((int)(((byte)(35)))));
            this.vLatestLabel.Location = new System.Drawing.Point(143, 145);
            this.vLatestLabel.Name = "vLatestLabel";
            this.vLatestLabel.Size = new System.Drawing.Size(37, 18);
            this.vLatestLabel.TabIndex = 8;
            this.vLatestLabel.Text = "x.x.x";
            // 
            // skipButton
            // 
            this.skipButton.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(40)))), ((int)(((byte)(40)))), ((int)(((byte)(40)))));
            this.skipButton.FlatStyle = System.Windows.Forms.FlatStyle.Flat;
            this.skipButton.Font = new System.Drawing.Font("Arial", 9F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.skipButton.ForeColor = System.Drawing.SystemColors.ButtonHighlight;
            this.skipButton.Location = new System.Drawing.Point(383, 230);
            this.skipButton.Name = "skipButton";
            this.skipButton.Size = new System.Drawing.Size(75, 26);
            this.skipButton.TabIndex = 7;
            this.skipButton.Text = "Ertele";
            this.skipButton.UseVisualStyleBackColor = false;
            this.skipButton.Click += new System.EventHandler(this.button3_Click);
            // 
            // noUpdateButton
            // 
            this.noUpdateButton.AutoSize = true;
            this.noUpdateButton.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(40)))), ((int)(((byte)(40)))), ((int)(((byte)(40)))));
            this.noUpdateButton.FlatStyle = System.Windows.Forms.FlatStyle.Flat;
            this.noUpdateButton.Font = new System.Drawing.Font("Arial", 9F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.noUpdateButton.ForeColor = System.Drawing.SystemColors.ButtonHighlight;
            this.noUpdateButton.Location = new System.Drawing.Point(283, 230);
            this.noUpdateButton.Name = "noUpdateButton";
            this.noUpdateButton.Size = new System.Drawing.Size(75, 27);
            this.noUpdateButton.TabIndex = 6;
            this.noUpdateButton.Text = "Hayır";
            this.noUpdateButton.UseVisualStyleBackColor = false;
            this.noUpdateButton.Click += new System.EventHandler(this.button2_Click);
            // 
            // updateButton
            // 
            this.updateButton.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(40)))), ((int)(((byte)(40)))), ((int)(((byte)(40)))));
            this.updateButton.FlatStyle = System.Windows.Forms.FlatStyle.Flat;
            this.updateButton.Font = new System.Drawing.Font("Arial", 9F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.updateButton.ForeColor = System.Drawing.SystemColors.ButtonHighlight;
            this.updateButton.Location = new System.Drawing.Point(183, 230);
            this.updateButton.Name = "updateButton";
            this.updateButton.Size = new System.Drawing.Size(75, 26);
            this.updateButton.TabIndex = 5;
            this.updateButton.Text = "Evet";
            this.updateButton.UseVisualStyleBackColor = false;
            this.updateButton.Click += new System.EventHandler(this.button1_Click);
            // 
            // vLatestLabelT
            // 
            this.vLatestLabelT.AutoSize = true;
            this.vLatestLabelT.BackColor = System.Drawing.Color.Transparent;
            this.vLatestLabelT.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.vLatestLabelT.ForeColor = System.Drawing.SystemColors.ButtonHighlight;
            this.vLatestLabelT.Location = new System.Drawing.Point(27, 145);
            this.vLatestLabelT.Name = "vLatestLabelT";
            this.vLatestLabelT.Size = new System.Drawing.Size(110, 18);
            this.vLatestLabelT.TabIndex = 4;
            this.vLatestLabelT.Text = "Güncel sürüm: ";
            // 
            // vCurrentLabelT
            // 
            this.vCurrentLabelT.AutoSize = true;
            this.vCurrentLabelT.BackColor = System.Drawing.Color.Transparent;
            this.vCurrentLabelT.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.vCurrentLabelT.ForeColor = System.Drawing.SystemColors.ButtonHighlight;
            this.vCurrentLabelT.Location = new System.Drawing.Point(27, 106);
            this.vCurrentLabelT.Name = "vCurrentLabelT";
            this.vCurrentLabelT.Size = new System.Drawing.Size(111, 18);
            this.vCurrentLabelT.TabIndex = 3;
            this.vCurrentLabelT.Text = "Mevcut sürüm: ";
            // 
            // updateLabelBottom
            // 
            this.updateLabelBottom.AutoSize = true;
            this.updateLabelBottom.BackColor = System.Drawing.Color.Transparent;
            this.updateLabelBottom.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.updateLabelBottom.ForeColor = System.Drawing.SystemColors.ButtonHighlight;
            this.updateLabelBottom.Location = new System.Drawing.Point(27, 182);
            this.updateLabelBottom.Name = "updateLabelBottom";
            this.updateLabelBottom.Size = new System.Drawing.Size(211, 18);
            this.updateLabelBottom.TabIndex = 2;
            this.updateLabelBottom.Text = "Yeni sürüme güncellensin mi?";
            // 
            // updateLabelTop
            // 
            this.updateLabelTop.AutoSize = true;
            this.updateLabelTop.BackColor = System.Drawing.Color.Transparent;
            this.updateLabelTop.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.updateLabelTop.ForeColor = System.Drawing.SystemColors.ButtonHighlight;
            this.updateLabelTop.Location = new System.Drawing.Point(27, 68);
            this.updateLabelTop.Name = "updateLabelTop";
            this.updateLabelTop.Size = new System.Drawing.Size(329, 18);
            this.updateLabelTop.TabIndex = 1;
            this.updateLabelTop.Text = "Projects başlatıcısı için yeni sürüm yayınlanmış!";
            // 
            // updateHeadline
            // 
            this.updateHeadline.AutoSize = true;
            this.updateHeadline.BackColor = System.Drawing.Color.Transparent;
            this.updateHeadline.Font = new System.Drawing.Font("Arial", 15.75F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.updateHeadline.ForeColor = System.Drawing.SystemColors.ButtonHighlight;
            this.updateHeadline.Location = new System.Drawing.Point(250, 17);
            this.updateHeadline.Name = "updateHeadline";
            this.updateHeadline.Size = new System.Drawing.Size(115, 24);
            this.updateHeadline.TabIndex = 0;
            this.updateHeadline.Text = "Yeni Sürüm";
            // 
            // panelYenilikler
            // 
            this.panelYenilikler.BackColor = System.Drawing.Color.Transparent;
            this.panelYenilikler.Controls.Add(this.versionLabel);
            this.panelYenilikler.Controls.Add(this.guna2PictureBox2);
            this.panelYenilikler.Controls.Add(this.labelYenilikMaddeler);
            this.panelYenilikler.Location = new System.Drawing.Point(0, 34);
            this.panelYenilikler.MaximumSize = new System.Drawing.Size(980, 470);
            this.panelYenilikler.MinimumSize = new System.Drawing.Size(980, 470);
            this.panelYenilikler.Name = "panelYenilikler";
            this.panelYenilikler.Size = new System.Drawing.Size(980, 470);
            this.panelYenilikler.TabIndex = 1023;
            this.panelYenilikler.Visible = false;
            // 
            // versionLabel
            // 
            this.versionLabel.AutoSize = true;
            this.versionLabel.BackColor = System.Drawing.Color.Transparent;
            this.versionLabel.Font = new System.Drawing.Font("Microsoft Sans Serif", 10F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.versionLabel.ForeColor = System.Drawing.SystemColors.ButtonFace;
            this.versionLabel.Location = new System.Drawing.Point(402, 40);
            this.versionLabel.Name = "versionLabel";
            this.versionLabel.Size = new System.Drawing.Size(47, 17);
            this.versionLabel.TabIndex = 1017;
            this.versionLabel.Text = "v0.1.8";
            // 
            // guna2PictureBox2
            // 
            this.guna2PictureBox2.Image = ((System.Drawing.Image)(resources.GetObject("guna2PictureBox2.Image")));
            this.guna2PictureBox2.ImageRotate = 0F;
            this.guna2PictureBox2.Location = new System.Drawing.Point(405, 12);
            this.guna2PictureBox2.Name = "guna2PictureBox2";
            this.guna2PictureBox2.Size = new System.Drawing.Size(191, 25);
            this.guna2PictureBox2.TabIndex = 1016;
            this.guna2PictureBox2.TabStop = false;
            // 
            // labelYenilikMaddeler
            // 
            this.labelYenilikMaddeler.AutoSize = true;
            this.labelYenilikMaddeler.BackColor = System.Drawing.Color.Transparent;
            this.labelYenilikMaddeler.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.labelYenilikMaddeler.ForeColor = System.Drawing.SystemColors.ButtonHighlight;
            this.labelYenilikMaddeler.Location = new System.Drawing.Point(22, 78);
            this.labelYenilikMaddeler.Name = "labelYenilikMaddeler";
            this.labelYenilikMaddeler.Size = new System.Drawing.Size(74, 252);
            this.labelYenilikMaddeler.TabIndex = 199;
            this.labelYenilikMaddeler.Text = "◣ vx.x.x ◢\r\n\r\n⊳ x\r\n\r\n⊳ x\r\n\r\n⊳ x\r\n\r\n⊳ x\r\n\r\n\r\n◣ vx.x.x ◢\r\n\r\n⊳ x\r\n";
            // 
            // loginMenuForm
            // 
            this.AutoScaleDimensions = new System.Drawing.SizeF(6F, 13F);
            this.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
            this.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.BackgroundImage = ((System.Drawing.Image)(resources.GetObject("$this.BackgroundImage")));
            this.BackgroundImageLayout = System.Windows.Forms.ImageLayout.Stretch;
            this.ClientSize = new System.Drawing.Size(980, 503);
            this.Controls.Add(this.panelYenilikler);
            this.Controls.Add(this.newVersionPanel);
            this.Controls.Add(this.loginButton);
            this.Controls.Add(this.webbutton);
            this.Controls.Add(this.newsLabel);
            this.Controls.Add(this.backButton);
            this.Controls.Add(this.updateNowButton);
            this.Controls.Add(this.maximizeButtonControlBox);
            this.Controls.Add(this.minimizeButtonControlBox);
            this.Controls.Add(this.raminfo);
            this.Controls.Add(this.rememberMeCheckBox);
            this.Controls.Add(this.nickNameEnterTextBox);
            this.Controls.Add(this.closeButtonControlBox);
            this.DoubleBuffered = true;
            this.FormBorderStyle = System.Windows.Forms.FormBorderStyle.None;
            this.Icon = ((System.Drawing.Icon)(resources.GetObject("$this.Icon")));
            this.MaximumSize = new System.Drawing.Size(980, 503);
            this.MinimumSize = new System.Drawing.Size(980, 503);
            this.Name = "loginMenuForm";
            this.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
            this.Text = "Projects Launcher";
            this.Load += new System.EventHandler(this.ProjectsLauncherLogin_Load);
            this.newVersionPanel.ResumeLayout(false);
            this.newVersionPanel.PerformLayout();
            this.panelYenilikler.ResumeLayout(false);
            this.panelYenilikler.PerformLayout();
            ((System.ComponentModel.ISupportInitialize)(this.guna2PictureBox2)).EndInit();
            this.ResumeLayout(false);
            this.PerformLayout();

        }

        #endregion
        private Guna.UI2.WinForms.Guna2BorderlessForm guna2BorderlessForm1;
        private System.Windows.Forms.Label raminfo;
        private Guna.UI2.WinForms.Guna2CheckBox rememberMeCheckBox;
        private Guna.UI2.WinForms.Guna2TextBox nickNameEnterTextBox;
        private Guna.UI2.WinForms.Guna2ControlBox closeButtonControlBox;
        private Guna.UI2.WinForms.Guna2ControlBox minimizeButtonControlBox;
        private Guna.UI2.WinForms.Guna2BorderlessForm guna2BorderlessForm2;
        private Guna.UI2.WinForms.Guna2ControlBox maximizeButtonControlBox;
        private System.Windows.Forms.Button updateNowButton;
        private System.Windows.Forms.Button backButton;
        private Guna.UI2.WinForms.Guna2ImageButton loginButton;
        private Guna.UI2.WinForms.Guna2ImageButton webbutton;
        private Guna.UI2.WinForms.Guna2ImageButton newsLabel;
        private Guna.UI2.WinForms.Guna2Panel panelYenilikler;
        private System.Windows.Forms.Label versionLabel;
        private Guna.UI2.WinForms.Guna2PictureBox guna2PictureBox2;
        private System.Windows.Forms.Label labelYenilikMaddeler;
        private System.Windows.Forms.Panel newVersionPanel;
        private System.Windows.Forms.Label vCurrentLabel;
        private System.Windows.Forms.Label vLatestLabel;
        private System.Windows.Forms.Button skipButton;
        private System.Windows.Forms.Button noUpdateButton;
        private System.Windows.Forms.Button updateButton;
        private System.Windows.Forms.Label vLatestLabelT;
        private System.Windows.Forms.Label vCurrentLabelT;
        private System.Windows.Forms.Label updateLabelBottom;
        private System.Windows.Forms.Label updateLabelTop;
        private System.Windows.Forms.Label updateHeadline;
    }
}

