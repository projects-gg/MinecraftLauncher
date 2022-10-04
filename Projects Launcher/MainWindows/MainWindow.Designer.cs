
using System.Linq;

namespace Projects_Launcher.Projects_Launcher
{
    partial class mainMenuForm
    {
        /// <summary>
        /// Required designer variable.
        /// </summary>
        private System.ComponentModel.IContainer components = null;

        /// <summary>
        /// Clean up any resources being used.
        /// </summary>
        /// <param name="disposing">true if managed resources should be disposed; otherwise, false.</param>
        protected override void Dispose(bool disposing)
        {
            if (disposing && (components != null))
            {
                components.Dispose();
            }
            base.Dispose(disposing);
        }

        #region Windows Form Designer generated code

        /*public string latestFabric =
            readPhpContent("https://mc.projects.gg/LauncherUpdateStream/version-fabric.php");*/

        /// <summary>
        /// Required method for Designer support - do not modify
        /// the contents of this method with the code editor.
        /// </summary>
        ///

        private void InitializeComponent()
        {
            this.components = new System.ComponentModel.Container();
            System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(mainMenuForm));
            this.prepareGameToLaunch = new System.Windows.Forms.Timer(this.components);
            this.timer3 = new System.Windows.Forms.Timer(this.components);
            this.guna2BorderlessForm1 = new Guna.UI2.WinForms.Guna2BorderlessForm(this.components);
            this.serverPing = new System.Windows.Forms.Timer(this.components);
            this.directorySearcher1 = new System.DirectoryServices.DirectorySearcher();
            this.Panel = new Guna.UI2.WinForms.Guna2Panel();
            this.guna2Panel3 = new Guna.UI2.WinForms.Guna2Panel();
            this.playerNameStaticLabell = new System.Windows.Forms.Label();
            this.skinRenderPictureBox = new Guna.UI2.WinForms.Guna2CirclePictureBox();
            this.playerNameStaticLabel = new System.Windows.Forms.Label();
            this.guna2Panel4 = new Guna.UI2.WinForms.Guna2Panel();
            this.guna2PictureBox10 = new Guna.UI2.WinForms.Guna2PictureBox();
            this.settingsStaticPictureBox = new Guna.UI2.WinForms.Guna2ImageButton();
            this.instagramStaticPictureBox = new Guna.UI2.WinForms.Guna2ImageButton();
            this.discordStaticPictureBox = new Guna.UI2.WinForms.Guna2ImageButton();
            this.mailStaticPictureBox = new Guna.UI2.WinForms.Guna2ImageButton();
            this.websiteStaticPictureBox = new Guna.UI2.WinForms.Guna2ImageButton();
            this.guna2Panel5 = new Guna.UI2.WinForms.Guna2Panel();
            this.PLauncherFC = new Guna.UI2.WinForms.Guna2ProgressBar();
            this.lobiOnline = new System.Windows.Forms.Label();
            this.label3 = new System.Windows.Forms.Label();
            this.serverOnlineCountStaticLabel = new System.Windows.Forms.Label();
            this.label2 = new System.Windows.Forms.Label();
            this.playButtonStaticLabel = new Guna.UI2.WinForms.Guna2Button();
            this.versionInfoStaticLabel = new System.Windows.Forms.Label();
            this.downloadCompleteBar = new Guna.UI2.WinForms.Guna2ProgressBar();
            this.playSplitStaticLabel = new System.Windows.Forms.Label();
            this.settingsBgPanel = new Guna.UI2.WinForms.Guna2TabControl();
            this.minecraftTabPage = new System.Windows.Forms.TabPage();
            this.versionBox = new Guna.UI2.WinForms.Guna2ComboBox();
            this.label9 = new System.Windows.Forms.Label();
            this.reopenLauncher = new Guna.UI2.WinForms.Guna2CheckBox();
            this.autoConnect = new Guna.UI2.WinForms.Guna2CheckBox();
            this.fullscreenCheckBox = new Guna.UI2.WinForms.Guna2CheckBox();
            this.heighttextbox = new Guna.UI2.WinForms.Guna2TextBox();
            this.label5 = new System.Windows.Forms.Label();
            this.widthtextbox = new Guna.UI2.WinForms.Guna2TextBox();
            this.label4 = new System.Windows.Forms.Label();
            this.javaTabPage = new System.Windows.Forms.TabPage();
            this.maxRamTextBox = new Guna.UI2.WinForms.Guna2TextBox();
            this.minRamTextBox = new Guna.UI2.WinForms.Guna2TextBox();
            this.ramInfoLabel = new System.Windows.Forms.Label();
            this.jvmTextBox = new Guna.UI2.WinForms.Guna2TextBox();
            this.label12 = new System.Windows.Forms.Label();
            this.maxRamDynamicCalculatorLabel = new System.Windows.Forms.Label();
            this.minRamMBtoGBLabel = new System.Windows.Forms.Label();
            this.label8 = new System.Windows.Forms.Label();
            this.label7 = new System.Windows.Forms.Label();
            this.label6 = new System.Windows.Forms.Label();
            this.tabPage1 = new System.Windows.Forms.TabPage();
            this.versionLabel = new System.Windows.Forms.Label();
            this.guna2TextBox4 = new Guna.UI2.WinForms.Guna2TextBox();
            this.label13 = new System.Windows.Forms.Label();
            this.maximizeButtonControlBox = new Guna.UI2.WinForms.Guna2ControlBox();
            this.label11 = new System.Windows.Forms.Label();
            this.closeButtonControlBox = new Guna.UI2.WinForms.Guna2ControlBox();
            this.minimizeButtonControlBox = new Guna.UI2.WinForms.Guna2ControlBox();
            this.ticksave = new System.Windows.Forms.Label();
            this.minramlabel = new System.Windows.Forms.Label();
            this.surumtext = new System.Windows.Forms.Label();
            this.widthlabel = new System.Windows.Forms.Label();
            this.heightlabel = new System.Windows.Forms.Label();
            this.maxramlabel = new System.Windows.Forms.Label();
            this.Panel.SuspendLayout();
            this.guna2Panel3.SuspendLayout();
            ((System.ComponentModel.ISupportInitialize)(this.skinRenderPictureBox)).BeginInit();
            this.guna2Panel4.SuspendLayout();
            ((System.ComponentModel.ISupportInitialize)(this.guna2PictureBox10)).BeginInit();
            this.guna2Panel5.SuspendLayout();
            this.settingsBgPanel.SuspendLayout();
            this.minecraftTabPage.SuspendLayout();
            this.javaTabPage.SuspendLayout();
            this.tabPage1.SuspendLayout();
            this.SuspendLayout();
            // 
            // prepareGameToLaunch
            // 
            this.prepareGameToLaunch.Tick += new System.EventHandler(this.prepareGameToLaunch_Tick);
            // 
            // timer3
            // 
            this.timer3.Tick += new System.EventHandler(this.timer3_Tick);
            // 
            // guna2BorderlessForm1
            // 
            this.guna2BorderlessForm1.ContainerControl = this;
            this.guna2BorderlessForm1.DockIndicatorTransparencyValue = 0.6D;
            this.guna2BorderlessForm1.TransparentWhileDrag = true;
            // 
            // serverPing
            // 
            this.serverPing.Enabled = true;
            this.serverPing.Interval = 5000;
            this.serverPing.Tick += new System.EventHandler(this.timer1_Tick);
            // 
            // directorySearcher1
            // 
            this.directorySearcher1.ClientTimeout = System.TimeSpan.Parse("-00:00:01");
            this.directorySearcher1.ServerPageTimeLimit = System.TimeSpan.Parse("-00:00:01");
            this.directorySearcher1.ServerTimeLimit = System.TimeSpan.Parse("-00:00:01");
            // 
            // Panel
            // 
            this.Panel.Anchor = ((System.Windows.Forms.AnchorStyles)((((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Bottom) 
            | System.Windows.Forms.AnchorStyles.Left) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.Panel.BackColor = System.Drawing.Color.Transparent;
            this.Panel.Controls.Add(this.guna2Panel3);
            this.Panel.Controls.Add(this.guna2Panel4);
            this.Panel.Controls.Add(this.guna2Panel5);
            this.Panel.Location = new System.Drawing.Point(0, 22);
            this.Panel.Name = "Panel";
            this.Panel.Size = new System.Drawing.Size(1025, 520);
            this.Panel.TabIndex = 1037;
            // 
            // guna2Panel3
            // 
            this.guna2Panel3.BackColor = System.Drawing.Color.Transparent;
            this.guna2Panel3.Controls.Add(this.playerNameStaticLabell);
            this.guna2Panel3.Controls.Add(this.skinRenderPictureBox);
            this.guna2Panel3.Controls.Add(this.playerNameStaticLabel);
            this.guna2Panel3.Dock = System.Windows.Forms.DockStyle.Left;
            this.guna2Panel3.Location = new System.Drawing.Point(0, 0);
            this.guna2Panel3.Name = "guna2Panel3";
            this.guna2Panel3.Size = new System.Drawing.Size(242, 391);
            this.guna2Panel3.TabIndex = 1027;
            // 
            // playerNameStaticLabell
            // 
            this.playerNameStaticLabell.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Right)));
            this.playerNameStaticLabell.AutoSize = true;
            this.playerNameStaticLabell.BackColor = System.Drawing.Color.Transparent;
            this.playerNameStaticLabell.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Bold);
            this.playerNameStaticLabell.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.playerNameStaticLabell.Location = new System.Drawing.Point(114, 51);
            this.playerNameStaticLabell.Name = "playerNameStaticLabell";
            this.playerNameStaticLabell.RightToLeft = System.Windows.Forms.RightToLeft.Yes;
            this.playerNameStaticLabell.Size = new System.Drawing.Size(63, 19);
            this.playerNameStaticLabell.TabIndex = 1032;
            this.playerNameStaticLabell.Text = "Kumak";
            this.playerNameStaticLabell.TextAlign = System.Drawing.ContentAlignment.MiddleRight;
            // 
            // skinRenderPictureBox
            // 
            this.skinRenderPictureBox.BackColor = System.Drawing.Color.Transparent;
            this.skinRenderPictureBox.FillColor = System.Drawing.Color.Transparent;
            this.skinRenderPictureBox.Image = ((System.Drawing.Image)(resources.GetObject("skinRenderPictureBox.Image")));
            this.skinRenderPictureBox.ImageRotate = 0F;
            this.skinRenderPictureBox.Location = new System.Drawing.Point(53, 31);
            this.skinRenderPictureBox.Name = "skinRenderPictureBox";
            this.skinRenderPictureBox.ShadowDecoration.Mode = Guna.UI2.WinForms.Enums.ShadowMode.Circle;
            this.skinRenderPictureBox.Size = new System.Drawing.Size(55, 55);
            this.skinRenderPictureBox.SizeMode = System.Windows.Forms.PictureBoxSizeMode.StretchImage;
            this.skinRenderPictureBox.TabIndex = 1030;
            this.skinRenderPictureBox.TabStop = false;
            this.skinRenderPictureBox.UseTransparentBackground = true;
            // 
            // playerNameStaticLabel
            // 
            this.playerNameStaticLabel.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Right)));
            this.playerNameStaticLabel.AutoSize = true;
            this.playerNameStaticLabel.BackColor = System.Drawing.Color.Transparent;
            this.playerNameStaticLabel.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Bold);
            this.playerNameStaticLabel.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.playerNameStaticLabel.Location = new System.Drawing.Point(119, -72);
            this.playerNameStaticLabel.Name = "playerNameStaticLabel";
            this.playerNameStaticLabel.RightToLeft = System.Windows.Forms.RightToLeft.Yes;
            this.playerNameStaticLabel.Size = new System.Drawing.Size(63, 19);
            this.playerNameStaticLabel.TabIndex = 1029;
            this.playerNameStaticLabel.Text = "Kumak";
            this.playerNameStaticLabel.TextAlign = System.Drawing.ContentAlignment.MiddleRight;
            // 
            // guna2Panel4
            // 
            this.guna2Panel4.BackColor = System.Drawing.Color.Transparent;
            this.guna2Panel4.Controls.Add(this.guna2PictureBox10);
            this.guna2Panel4.Controls.Add(this.settingsStaticPictureBox);
            this.guna2Panel4.Controls.Add(this.instagramStaticPictureBox);
            this.guna2Panel4.Controls.Add(this.discordStaticPictureBox);
            this.guna2Panel4.Controls.Add(this.mailStaticPictureBox);
            this.guna2Panel4.Controls.Add(this.websiteStaticPictureBox);
            this.guna2Panel4.Dock = System.Windows.Forms.DockStyle.Right;
            this.guna2Panel4.Location = new System.Drawing.Point(783, 0);
            this.guna2Panel4.Name = "guna2Panel4";
            this.guna2Panel4.Size = new System.Drawing.Size(242, 391);
            this.guna2Panel4.TabIndex = 1027;
            // 
            // guna2PictureBox10
            // 
            this.guna2PictureBox10.BackColor = System.Drawing.Color.Transparent;
            this.guna2PictureBox10.BackgroundImage = global::Projects_Launcher.Properties.Resources.projects_Logo;
            this.guna2PictureBox10.BackgroundImageLayout = System.Windows.Forms.ImageLayout.Stretch;
            this.guna2PictureBox10.FillColor = System.Drawing.Color.Transparent;
            this.guna2PictureBox10.ImageRotate = 0F;
            this.guna2PictureBox10.Location = new System.Drawing.Point(89, 42);
            this.guna2PictureBox10.Name = "guna2PictureBox10";
            this.guna2PictureBox10.Size = new System.Drawing.Size(65, 65);
            this.guna2PictureBox10.SizeMode = System.Windows.Forms.PictureBoxSizeMode.StretchImage;
            this.guna2PictureBox10.TabIndex = 1032;
            this.guna2PictureBox10.TabStop = false;
            this.guna2PictureBox10.UseTransparentBackground = true;
            // 
            // settingsStaticPictureBox
            // 
            this.settingsStaticPictureBox.BackColor = System.Drawing.Color.Transparent;
            this.settingsStaticPictureBox.CheckedState.ImageSize = new System.Drawing.Size(64, 64);
            this.settingsStaticPictureBox.Cursor = System.Windows.Forms.Cursors.Hand;
            this.settingsStaticPictureBox.HoverState.Image = ((System.Drawing.Image)(resources.GetObject("resource.Image")));
            this.settingsStaticPictureBox.HoverState.ImageSize = new System.Drawing.Size(25, 25);
            this.settingsStaticPictureBox.Image = ((System.Drawing.Image)(resources.GetObject("settingsStaticPictureBox.Image")));
            this.settingsStaticPictureBox.ImageOffset = new System.Drawing.Point(0, 0);
            this.settingsStaticPictureBox.ImageRotate = 0F;
            this.settingsStaticPictureBox.ImageSize = new System.Drawing.Size(25, 25);
            this.settingsStaticPictureBox.Location = new System.Drawing.Point(108, 120);
            this.settingsStaticPictureBox.Name = "settingsStaticPictureBox";
            this.settingsStaticPictureBox.PressedState.ImageSize = new System.Drawing.Size(25, 25);
            this.settingsStaticPictureBox.Size = new System.Drawing.Size(25, 25);
            this.settingsStaticPictureBox.TabIndex = 1031;
            this.settingsStaticPictureBox.Click += new System.EventHandler(this.ayarlarbutton_Click);
            // 
            // instagramStaticPictureBox
            // 
            this.instagramStaticPictureBox.BackColor = System.Drawing.Color.Transparent;
            this.instagramStaticPictureBox.CheckedState.ImageSize = new System.Drawing.Size(64, 64);
            this.instagramStaticPictureBox.Cursor = System.Windows.Forms.Cursors.Hand;
            this.instagramStaticPictureBox.HoverState.Image = ((System.Drawing.Image)(resources.GetObject("resource.Image1")));
            this.instagramStaticPictureBox.HoverState.ImageSize = new System.Drawing.Size(25, 25);
            this.instagramStaticPictureBox.Image = ((System.Drawing.Image)(resources.GetObject("instagramStaticPictureBox.Image")));
            this.instagramStaticPictureBox.ImageOffset = new System.Drawing.Point(0, 0);
            this.instagramStaticPictureBox.ImageRotate = 0F;
            this.instagramStaticPictureBox.ImageSize = new System.Drawing.Size(25, 25);
            this.instagramStaticPictureBox.Location = new System.Drawing.Point(108, 244);
            this.instagramStaticPictureBox.Name = "instagramStaticPictureBox";
            this.instagramStaticPictureBox.PressedState.ImageSize = new System.Drawing.Size(25, 25);
            this.instagramStaticPictureBox.Size = new System.Drawing.Size(25, 25);
            this.instagramStaticPictureBox.TabIndex = 1031;
            // 
            // discordStaticPictureBox
            // 
            this.discordStaticPictureBox.BackColor = System.Drawing.Color.Transparent;
            this.discordStaticPictureBox.CheckedState.ImageSize = new System.Drawing.Size(64, 64);
            this.discordStaticPictureBox.Cursor = System.Windows.Forms.Cursors.Hand;
            this.discordStaticPictureBox.HoverState.Image = ((System.Drawing.Image)(resources.GetObject("resource.Image2")));
            this.discordStaticPictureBox.HoverState.ImageSize = new System.Drawing.Size(25, 25);
            this.discordStaticPictureBox.Image = ((System.Drawing.Image)(resources.GetObject("discordStaticPictureBox.Image")));
            this.discordStaticPictureBox.ImageOffset = new System.Drawing.Point(0, 0);
            this.discordStaticPictureBox.ImageRotate = 0F;
            this.discordStaticPictureBox.ImageSize = new System.Drawing.Size(25, 25);
            this.discordStaticPictureBox.Location = new System.Drawing.Point(108, 208);
            this.discordStaticPictureBox.Name = "discordStaticPictureBox";
            this.discordStaticPictureBox.PressedState.ImageSize = new System.Drawing.Size(25, 25);
            this.discordStaticPictureBox.Size = new System.Drawing.Size(25, 25);
            this.discordStaticPictureBox.TabIndex = 1030;
            // 
            // mailStaticPictureBox
            // 
            this.mailStaticPictureBox.BackColor = System.Drawing.Color.Transparent;
            this.mailStaticPictureBox.CheckedState.ImageSize = new System.Drawing.Size(64, 64);
            this.mailStaticPictureBox.Cursor = System.Windows.Forms.Cursors.Hand;
            this.mailStaticPictureBox.HoverState.Image = ((System.Drawing.Image)(resources.GetObject("resource.Image3")));
            this.mailStaticPictureBox.HoverState.ImageSize = new System.Drawing.Size(25, 25);
            this.mailStaticPictureBox.Image = ((System.Drawing.Image)(resources.GetObject("mailStaticPictureBox.Image")));
            this.mailStaticPictureBox.ImageOffset = new System.Drawing.Point(0, 0);
            this.mailStaticPictureBox.ImageRotate = 0F;
            this.mailStaticPictureBox.ImageSize = new System.Drawing.Size(25, 25);
            this.mailStaticPictureBox.Location = new System.Drawing.Point(108, 280);
            this.mailStaticPictureBox.Name = "mailStaticPictureBox";
            this.mailStaticPictureBox.PressedState.ImageSize = new System.Drawing.Size(25, 25);
            this.mailStaticPictureBox.Size = new System.Drawing.Size(25, 25);
            this.mailStaticPictureBox.TabIndex = 1029;
            this.mailStaticPictureBox.Click += new System.EventHandler(this.mailStaticPictureBox_Click);
            // 
            // websiteStaticPictureBox
            // 
            this.websiteStaticPictureBox.BackColor = System.Drawing.Color.Transparent;
            this.websiteStaticPictureBox.CheckedState.ImageSize = new System.Drawing.Size(64, 64);
            this.websiteStaticPictureBox.Cursor = System.Windows.Forms.Cursors.Hand;
            this.websiteStaticPictureBox.HoverState.Image = ((System.Drawing.Image)(resources.GetObject("resource.Image4")));
            this.websiteStaticPictureBox.HoverState.ImageSize = new System.Drawing.Size(25, 25);
            this.websiteStaticPictureBox.Image = ((System.Drawing.Image)(resources.GetObject("websiteStaticPictureBox.Image")));
            this.websiteStaticPictureBox.ImageOffset = new System.Drawing.Point(0, 0);
            this.websiteStaticPictureBox.ImageRotate = 0F;
            this.websiteStaticPictureBox.ImageSize = new System.Drawing.Size(25, 25);
            this.websiteStaticPictureBox.Location = new System.Drawing.Point(108, 172);
            this.websiteStaticPictureBox.Name = "websiteStaticPictureBox";
            this.websiteStaticPictureBox.PressedState.ImageSize = new System.Drawing.Size(25, 25);
            this.websiteStaticPictureBox.Size = new System.Drawing.Size(25, 25);
            this.websiteStaticPictureBox.TabIndex = 1028;
            // 
            // guna2Panel5
            // 
            this.guna2Panel5.AutoSize = true;
            this.guna2Panel5.BackColor = System.Drawing.Color.Transparent;
            this.guna2Panel5.Controls.Add(this.PLauncherFC);
            this.guna2Panel5.Controls.Add(this.lobiOnline);
            this.guna2Panel5.Controls.Add(this.label3);
            this.guna2Panel5.Controls.Add(this.serverOnlineCountStaticLabel);
            this.guna2Panel5.Controls.Add(this.label2);
            this.guna2Panel5.Controls.Add(this.playButtonStaticLabel);
            this.guna2Panel5.Controls.Add(this.versionInfoStaticLabel);
            this.guna2Panel5.Controls.Add(this.downloadCompleteBar);
            this.guna2Panel5.Controls.Add(this.playSplitStaticLabel);
            this.guna2Panel5.Dock = System.Windows.Forms.DockStyle.Bottom;
            this.guna2Panel5.Location = new System.Drawing.Point(0, 391);
            this.guna2Panel5.Name = "guna2Panel5";
            this.guna2Panel5.Size = new System.Drawing.Size(1025, 129);
            this.guna2Panel5.TabIndex = 1027;
            // 
            // PLauncherFC
            // 
            this.PLauncherFC.BorderRadius = 3;
            this.PLauncherFC.Font = new System.Drawing.Font("Arial", 7F);
            this.PLauncherFC.Location = new System.Drawing.Point(53, 34);
            this.PLauncherFC.Name = "PLauncherFC";
            this.PLauncherFC.ProgressColor = System.Drawing.Color.FromArgb(((int)(((byte)(22)))), ((int)(((byte)(163)))), ((int)(((byte)(74)))));
            this.PLauncherFC.ProgressColor2 = System.Drawing.Color.FromArgb(((int)(((byte)(22)))), ((int)(((byte)(163)))), ((int)(((byte)(74)))));
            this.PLauncherFC.ShowText = true;
            this.PLauncherFC.Size = new System.Drawing.Size(100, 10);
            this.PLauncherFC.TabIndex = 1034;
            this.PLauncherFC.Text = "guna2ProgressBar1";
            this.PLauncherFC.TextRenderingHint = System.Drawing.Text.TextRenderingHint.SystemDefault;
            this.PLauncherFC.Visible = false;
            // 
            // lobiOnline
            // 
            this.lobiOnline.Anchor = ((System.Windows.Forms.AnchorStyles)(((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Bottom) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.lobiOnline.AutoSize = true;
            this.lobiOnline.BackColor = System.Drawing.Color.Transparent;
            this.lobiOnline.Font = new System.Drawing.Font("Arial Black", 12F, System.Drawing.FontStyle.Bold, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.lobiOnline.ForeColor = System.Drawing.Color.SaddleBrown;
            this.lobiOnline.Location = new System.Drawing.Point(848, 77);
            this.lobiOnline.Name = "lobiOnline";
            this.lobiOnline.Size = new System.Drawing.Size(20, 23);
            this.lobiOnline.TabIndex = 1032;
            this.lobiOnline.Text = "⋄";
            // 
            // label3
            // 
            this.label3.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.label3.AutoSize = true;
            this.label3.BackColor = System.Drawing.Color.Transparent;
            this.label3.Font = new System.Drawing.Font("Arial", 9F, System.Drawing.FontStyle.Bold, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label3.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(19)))), ((int)(((byte)(135)))), ((int)(((byte)(195)))));
            this.label3.Location = new System.Drawing.Point(818, 82);
            this.label3.Name = "label3";
            this.label3.Size = new System.Drawing.Size(34, 15);
            this.label3.TabIndex = 1031;
            this.label3.Text = "LOBİ";
            // 
            // serverOnlineCountStaticLabel
            // 
            this.serverOnlineCountStaticLabel.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.serverOnlineCountStaticLabel.AutoSize = true;
            this.serverOnlineCountStaticLabel.BackColor = System.Drawing.Color.Transparent;
            this.serverOnlineCountStaticLabel.Font = new System.Drawing.Font("Arial", 9F, System.Drawing.FontStyle.Bold, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.serverOnlineCountStaticLabel.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(19)))), ((int)(((byte)(135)))), ((int)(((byte)(195)))));
            this.serverOnlineCountStaticLabel.Location = new System.Drawing.Point(889, 82);
            this.serverOnlineCountStaticLabel.Name = "serverOnlineCountStaticLabel";
            this.serverOnlineCountStaticLabel.Size = new System.Drawing.Size(83, 15);
            this.serverOnlineCountStaticLabel.TabIndex = 1030;
            this.serverOnlineCountStaticLabel.Text = "000 Kişi aktif!";
            // 
            // label2
            // 
            this.label2.AutoSize = true;
            this.label2.BackColor = System.Drawing.Color.Transparent;
            this.label2.Font = new System.Drawing.Font("Microsoft Sans Serif", 19F, System.Drawing.FontStyle.Bold);
            this.label2.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(20)))), ((int)(((byte)(135)))), ((int)(((byte)(145)))));
            this.label2.Location = new System.Drawing.Point(867, 73);
            this.label2.Name = "label2";
            this.label2.Size = new System.Drawing.Size(21, 30);
            this.label2.TabIndex = 1029;
            this.label2.Text = "|";
            // 
            // playButtonStaticLabel
            // 
            this.playButtonStaticLabel.BackColor = System.Drawing.Color.Transparent;
            this.playButtonStaticLabel.BorderRadius = 10;
            this.playButtonStaticLabel.DisabledState.BorderColor = System.Drawing.Color.DarkGray;
            this.playButtonStaticLabel.DisabledState.CustomBorderColor = System.Drawing.Color.DarkGray;
            this.playButtonStaticLabel.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(169)))), ((int)(((byte)(169)))), ((int)(((byte)(169)))));
            this.playButtonStaticLabel.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(141)))), ((int)(((byte)(141)))), ((int)(((byte)(141)))));
            this.playButtonStaticLabel.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(19)))), ((int)(((byte)(115)))), ((int)(((byte)(175)))));
            this.playButtonStaticLabel.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.playButtonStaticLabel.ForeColor = System.Drawing.Color.White;
            this.playButtonStaticLabel.Location = new System.Drawing.Point(53, 66);
            this.playButtonStaticLabel.Name = "playButtonStaticLabel";
            this.playButtonStaticLabel.Size = new System.Drawing.Size(100, 30);
            this.playButtonStaticLabel.TabIndex = 1028;
            this.playButtonStaticLabel.Text = "OYNA";
            this.playButtonStaticLabel.Click += new System.EventHandler(this.oynabutton_Click);
            // 
            // versionInfoStaticLabel
            // 
            this.versionInfoStaticLabel.AutoSize = true;
            this.versionInfoStaticLabel.BackColor = System.Drawing.Color.Transparent;
            this.versionInfoStaticLabel.Font = new System.Drawing.Font("Arial", 9F, System.Drawing.FontStyle.Bold, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.versionInfoStaticLabel.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(19)))), ((int)(((byte)(135)))), ((int)(((byte)(195)))));
            this.versionInfoStaticLabel.Location = new System.Drawing.Point(182, 73);
            this.versionInfoStaticLabel.Name = "versionInfoStaticLabel";
            this.versionInfoStaticLabel.Size = new System.Drawing.Size(108, 15);
            this.versionInfoStaticLabel.TabIndex = 1027;
            this.versionInfoStaticLabel.Text = "projects-fabric-v1";
            // 
            // downloadCompleteBar
            // 
            this.downloadCompleteBar.BorderRadius = 3;
            this.downloadCompleteBar.Font = new System.Drawing.Font("Arial", 7F);
            this.downloadCompleteBar.Location = new System.Drawing.Point(53, 50);
            this.downloadCompleteBar.Name = "downloadCompleteBar";
            this.downloadCompleteBar.ProgressColor = System.Drawing.Color.FromArgb(((int)(((byte)(22)))), ((int)(((byte)(163)))), ((int)(((byte)(74)))));
            this.downloadCompleteBar.ProgressColor2 = System.Drawing.Color.FromArgb(((int)(((byte)(22)))), ((int)(((byte)(163)))), ((int)(((byte)(74)))));
            this.downloadCompleteBar.ShowText = true;
            this.downloadCompleteBar.Size = new System.Drawing.Size(100, 10);
            this.downloadCompleteBar.TabIndex = 1026;
            this.downloadCompleteBar.Text = "downloadCompleteBar";
            this.downloadCompleteBar.TextRenderingHint = System.Drawing.Text.TextRenderingHint.SystemDefault;
            this.downloadCompleteBar.Visible = false;

            // 
            // playSplitStaticLabel
            // 
            this.playSplitStaticLabel.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left)));
            this.playSplitStaticLabel.AutoSize = true;
            this.playSplitStaticLabel.BackColor = System.Drawing.Color.Transparent;
            this.playSplitStaticLabel.Font = new System.Drawing.Font("Microsoft Sans Serif", 19F, System.Drawing.FontStyle.Bold);
            this.playSplitStaticLabel.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(20)))), ((int)(((byte)(135)))), ((int)(((byte)(145)))));
            this.playSplitStaticLabel.Location = new System.Drawing.Point(160, 64);
            this.playSplitStaticLabel.Name = "playSplitStaticLabel";
            this.playSplitStaticLabel.Size = new System.Drawing.Size(21, 30);
            this.playSplitStaticLabel.TabIndex = 1024;
            this.playSplitStaticLabel.Text = "|";
            // 
            // settingsBgPanel
            // 
            this.settingsBgPanel.Controls.Add(this.minecraftTabPage);
            this.settingsBgPanel.Controls.Add(this.javaTabPage);
            this.settingsBgPanel.Controls.Add(this.tabPage1);
            this.settingsBgPanel.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.settingsBgPanel.ItemSize = new System.Drawing.Size(100, 30);
            this.settingsBgPanel.Location = new System.Drawing.Point(248, 26);
            this.settingsBgPanel.Name = "settingsBgPanel";
            this.settingsBgPanel.SelectedIndex = 0;
            this.settingsBgPanel.Size = new System.Drawing.Size(529, 392);
            this.settingsBgPanel.TabButtonHoverState.BorderColor = System.Drawing.Color.Empty;
            this.settingsBgPanel.TabButtonHoverState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(40)))), ((int)(((byte)(52)))), ((int)(((byte)(70)))));
            this.settingsBgPanel.TabButtonHoverState.Font = new System.Drawing.Font("Segoe UI Semibold", 10F);
            this.settingsBgPanel.TabButtonHoverState.ForeColor = System.Drawing.Color.White;
            this.settingsBgPanel.TabButtonHoverState.InnerColor = System.Drawing.Color.FromArgb(((int)(((byte)(40)))), ((int)(((byte)(52)))), ((int)(((byte)(70)))));
            this.settingsBgPanel.TabButtonIdleState.BorderColor = System.Drawing.Color.Empty;
            this.settingsBgPanel.TabButtonIdleState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(33)))), ((int)(((byte)(42)))), ((int)(((byte)(57)))));
            this.settingsBgPanel.TabButtonIdleState.Font = new System.Drawing.Font("Segoe UI Semibold", 10F);
            this.settingsBgPanel.TabButtonIdleState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(156)))), ((int)(((byte)(160)))), ((int)(((byte)(167)))));
            this.settingsBgPanel.TabButtonIdleState.InnerColor = System.Drawing.Color.FromArgb(((int)(((byte)(33)))), ((int)(((byte)(42)))), ((int)(((byte)(57)))));
            this.settingsBgPanel.TabButtonSelectedState.BorderColor = System.Drawing.Color.Empty;
            this.settingsBgPanel.TabButtonSelectedState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(78)))), ((int)(((byte)(111)))), ((int)(((byte)(136)))));
            this.settingsBgPanel.TabButtonSelectedState.Font = new System.Drawing.Font("Arial", 9F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.settingsBgPanel.TabButtonSelectedState.ForeColor = System.Drawing.Color.White;
            this.settingsBgPanel.TabButtonSelectedState.InnerColor = System.Drawing.Color.FromArgb(((int)(((byte)(76)))), ((int)(((byte)(132)))), ((int)(((byte)(255)))));
            this.settingsBgPanel.TabButtonSize = new System.Drawing.Size(100, 30);
            this.settingsBgPanel.TabIndex = 1036;
            this.settingsBgPanel.TabMenuBackColor = System.Drawing.Color.FromArgb(((int)(((byte)(60)))), ((int)(((byte)(90)))), ((int)(((byte)(114)))));
            this.settingsBgPanel.TabMenuOrientation = Guna.UI2.WinForms.TabMenuOrientation.HorizontalTop;
            this.settingsBgPanel.Visible = false;
            // 
            // minecraftTabPage
            // 
            this.minecraftTabPage.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(64)))), ((int)(((byte)(95)))), ((int)(((byte)(115)))));
            this.minecraftTabPage.Controls.Add(this.versionBox);
            this.minecraftTabPage.Controls.Add(this.label9);
            this.minecraftTabPage.Controls.Add(this.reopenLauncher);
            this.minecraftTabPage.Controls.Add(this.autoConnect);
            this.minecraftTabPage.Controls.Add(this.fullscreenCheckBox);
            this.minecraftTabPage.Controls.Add(this.heighttextbox);
            this.minecraftTabPage.Controls.Add(this.label5);
            this.minecraftTabPage.Controls.Add(this.widthtextbox);
            this.minecraftTabPage.Controls.Add(this.label4);
            this.minecraftTabPage.Location = new System.Drawing.Point(4, 34);
            this.minecraftTabPage.Name = "minecraftTabPage";
            this.minecraftTabPage.Padding = new System.Windows.Forms.Padding(3);
            this.minecraftTabPage.Size = new System.Drawing.Size(521, 354);
            this.minecraftTabPage.TabIndex = 3;
            this.minecraftTabPage.Text = "Minecraft";
            // 
            // versionBox
            // 
            this.versionBox.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(60)))), ((int)(((byte)(139)))), ((int)(((byte)(12)))));
            this.versionBox.BorderColor = System.Drawing.Color.WhiteSmoke;
            this.versionBox.DrawMode = System.Windows.Forms.DrawMode.OwnerDrawFixed;
            this.versionBox.DropDownStyle = System.Windows.Forms.ComboBoxStyle.DropDownList;
            this.versionBox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(60)))), ((int)(((byte)(90)))), ((int)(((byte)(114)))));
            this.versionBox.FocusedColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.versionBox.FocusedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.versionBox.Font = new System.Drawing.Font("Arial", 9.75F);
            this.versionBox.ForeColor = System.Drawing.Color.Black;
            this.versionBox.ItemHeight = 30;
            this.versionBox.Items.AddRange(new object[] {
            "projects-fabric-v1",
            "1.19.2",
            "1.19.1",
            "1.19",
            "1.18.2",
            "1.18.1",
            "1.18",
            "1.17.1",
            "1.17",
            "1.16.5",
            "1.16.4",
            "1.16.3",
            "1.16.2",
            "1.16.1",
            "1.16",
            "1.15.2",
            "1.15.1",
            "1.15",
            "1.14.4",
            "1.14.3",
            "1.14.2",
            "1.14.1",
            "1.14",
            "1.13.2",
            "1.13.1",
            "1.13",
            "1.12.2",
            "1.12.1",
            "1.12",
            "1.11.2",
            "1.11.1",
            "1.11",
            "1.10.2",
            "1.10.1",
            "1.10",
            "1.9.4",
            "1.9.3",
            "1.9.2",
            "1.9.1",
            "1.9",
            "1.8.9",
            "1.8.8",
            "1.8.7",
            "1.8.6",
            "1.8.5",
            "1.8.4",
            "1.8.3",
            "1.8.2",
            "1.8.1",
            "1.8",
            "1.7.10",
            "1.7.9",
            "1.7.8",
            "1.7.6",
            "1.7.5",
            "1.7.4",
            "1.7.3",
            "1.7.2",
            "1.7.1",
            "1.7",
            "1.6.4",
            "1.6.3",
            "1.6.2",
            "1.6.1",
            "1.6",
            "1.5.1",
            "1.5",
            "1.4.7",
            "1.4.6",
            "1.4.5",
            "1.4.4",
            "1.4.3",
            "1.4.2",
            "1.3.2",
            "1.3.1",
            "1.2.5",
            "1.2.4",
            "1.2.3",
            "1.2.2",
            "1.2.1",
            "1.1",
            "1.0"});
            this.versionBox.Location = new System.Drawing.Point(249, 37);
            this.versionBox.Name = "versionBox";
            this.versionBox.Size = new System.Drawing.Size(130, 36);
            this.versionBox.TabIndex = 1041;
            this.versionBox.SelectedIndexChanged += new System.EventHandler(this.surumsec_SelectedIndexChanged);
            // 
            // label9
            // 
            this.label9.AutoSize = true;
            this.label9.BackColor = System.Drawing.Color.Transparent;
            this.label9.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label9.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.label9.Location = new System.Drawing.Point(246, 11);
            this.label9.Name = "label9";
            this.label9.Size = new System.Drawing.Size(53, 18);
            this.label9.TabIndex = 1040;
            this.label9.Text = "Sürüm";
            // 
            // reopenLauncher
            // 
            this.reopenLauncher.AutoSize = true;
            this.reopenLauncher.CheckedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.reopenLauncher.CheckedState.BorderRadius = 0;
            this.reopenLauncher.CheckedState.BorderThickness = 0;
            this.reopenLauncher.CheckedState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.reopenLauncher.Font = new System.Drawing.Font("Arial", 9F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.reopenLauncher.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.reopenLauncher.Location = new System.Drawing.Point(12, 140);
            this.reopenLauncher.Name = "reopenLauncher";
            this.reopenLauncher.RightToLeft = System.Windows.Forms.RightToLeft.Yes;
            this.reopenLauncher.Size = new System.Drawing.Size(133, 19);
            this.reopenLauncher.TabIndex = 1039;
            this.reopenLauncher.Text = "Oyun Kapatılınca Aç";
            this.reopenLauncher.UncheckedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(125)))), ((int)(((byte)(137)))), ((int)(((byte)(149)))));
            this.reopenLauncher.UncheckedState.BorderRadius = 0;
            this.reopenLauncher.UncheckedState.BorderThickness = 0;
            this.reopenLauncher.UncheckedState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(125)))), ((int)(((byte)(137)))), ((int)(((byte)(149)))));
            this.reopenLauncher.CheckedChanged += new System.EventHandler(this.reopenLauncherCheckBox_MouseEnter);
            // 
            // autoConnect
            // 
            this.autoConnect.AutoSize = true;
            this.autoConnect.CheckedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.autoConnect.CheckedState.BorderRadius = 0;
            this.autoConnect.CheckedState.BorderThickness = 0;
            this.autoConnect.CheckedState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.autoConnect.Font = new System.Drawing.Font("Arial", 9F);
            this.autoConnect.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.autoConnect.Location = new System.Drawing.Point(12, 115);
            this.autoConnect.Name = "autoConnect";
            this.autoConnect.RightToLeft = System.Windows.Forms.RightToLeft.Yes;
            this.autoConnect.Size = new System.Drawing.Size(174, 19);
            this.autoConnect.TabIndex = 1038;
            this.autoConnect.Text = "Sunucuya Otomatik Bağlan";
            this.autoConnect.UncheckedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(125)))), ((int)(((byte)(137)))), ((int)(((byte)(149)))));
            this.autoConnect.UncheckedState.BorderRadius = 0;
            this.autoConnect.UncheckedState.BorderThickness = 0;
            this.autoConnect.UncheckedState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(125)))), ((int)(((byte)(137)))), ((int)(((byte)(149)))));
            this.autoConnect.CheckedChanged += new System.EventHandler(this.autoConnect_CheckedChanged);
            this.autoConnect.MouseEnter += new System.EventHandler(this.autoConnectCheckBox_MouseEnter);
            // 
            // fullscreenCheckBox
            // 
            this.fullscreenCheckBox.AutoSize = true;
            this.fullscreenCheckBox.CheckedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.fullscreenCheckBox.CheckedState.BorderRadius = 0;
            this.fullscreenCheckBox.CheckedState.BorderThickness = 0;
            this.fullscreenCheckBox.CheckedState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.fullscreenCheckBox.Font = new System.Drawing.Font("Arial", 9F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.fullscreenCheckBox.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.fullscreenCheckBox.Location = new System.Drawing.Point(12, 70);
            this.fullscreenCheckBox.Name = "fullscreenCheckBox";
            this.fullscreenCheckBox.RightToLeft = System.Windows.Forms.RightToLeft.Yes;
            this.fullscreenCheckBox.Size = new System.Drawing.Size(85, 19);
            this.fullscreenCheckBox.TabIndex = 1037;
            this.fullscreenCheckBox.Text = "Tam Ekran";
            this.fullscreenCheckBox.UncheckedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(125)))), ((int)(((byte)(137)))), ((int)(((byte)(149)))));
            this.fullscreenCheckBox.UncheckedState.BorderRadius = 0;
            this.fullscreenCheckBox.UncheckedState.BorderThickness = 0;
            this.fullscreenCheckBox.UncheckedState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(125)))), ((int)(((byte)(137)))), ((int)(((byte)(149)))));
            // 
            // heighttextbox
            // 
            this.heighttextbox.Animated = true;
            this.heighttextbox.BackColor = System.Drawing.Color.White;
            this.heighttextbox.BorderColor = System.Drawing.Color.WhiteSmoke;
            this.heighttextbox.BorderStyle = System.Drawing.Drawing2D.DashStyle.Custom;
            this.heighttextbox.Cursor = System.Windows.Forms.Cursors.IBeam;
            this.heighttextbox.DefaultText = "1920";
            this.heighttextbox.DisabledState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(208)))), ((int)(((byte)(208)))), ((int)(((byte)(208)))));
            this.heighttextbox.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(226)))), ((int)(((byte)(226)))), ((int)(((byte)(226)))));
            this.heighttextbox.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.heighttextbox.DisabledState.PlaceholderForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.heighttextbox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(64)))), ((int)(((byte)(95)))), ((int)(((byte)(115)))));
            this.heighttextbox.FocusedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.heighttextbox.Font = new System.Drawing.Font("Arial", 9F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.heighttextbox.ForeColor = System.Drawing.Color.Black;
            this.heighttextbox.HoverState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.heighttextbox.Location = new System.Drawing.Point(99, 36);
            this.heighttextbox.Name = "heighttextbox";
            this.heighttextbox.PasswordChar = '\0';
            this.heighttextbox.PlaceholderText = "";
            this.heighttextbox.SelectedText = "";
            this.heighttextbox.Size = new System.Drawing.Size(51, 29);
            this.heighttextbox.TabIndex = 1036;
            this.heighttextbox.WordWrap = false;
            this.heighttextbox.TextChanged += new System.EventHandler(this.heighttextbox_TextChanged);
            this.heighttextbox.Leave += new System.EventHandler(this.heighttextbox_Leave);
            // 
            // label5
            // 
            this.label5.AutoSize = true;
            this.label5.BackColor = System.Drawing.Color.Transparent;
            this.label5.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label5.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.label5.Location = new System.Drawing.Point(74, 40);
            this.label5.Name = "label5";
            this.label5.Size = new System.Drawing.Size(15, 18);
            this.label5.TabIndex = 1035;
            this.label5.Text = "x";
            // 
            // widthtextbox
            // 
            this.widthtextbox.Animated = true;
            this.widthtextbox.BackColor = System.Drawing.Color.White;
            this.widthtextbox.BorderColor = System.Drawing.Color.WhiteSmoke;
            this.widthtextbox.BorderStyle = System.Drawing.Drawing2D.DashStyle.Custom;
            this.widthtextbox.Cursor = System.Windows.Forms.Cursors.IBeam;
            this.widthtextbox.DefaultText = "1920";
            this.widthtextbox.DisabledState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(208)))), ((int)(((byte)(208)))), ((int)(((byte)(208)))));
            this.widthtextbox.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(226)))), ((int)(((byte)(226)))), ((int)(((byte)(226)))));
            this.widthtextbox.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.widthtextbox.DisabledState.PlaceholderForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.widthtextbox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(64)))), ((int)(((byte)(95)))), ((int)(((byte)(115)))));
            this.widthtextbox.FocusedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.widthtextbox.Font = new System.Drawing.Font("Arial", 9F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.widthtextbox.ForeColor = System.Drawing.Color.Black;
            this.widthtextbox.HoverState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.widthtextbox.Location = new System.Drawing.Point(12, 36);
            this.widthtextbox.Name = "widthtextbox";
            this.widthtextbox.PasswordChar = '\0';
            this.widthtextbox.PlaceholderText = "";
            this.widthtextbox.SelectedText = "";
            this.widthtextbox.Size = new System.Drawing.Size(51, 29);
            this.widthtextbox.TabIndex = 1034;
            this.widthtextbox.WordWrap = false;
            this.widthtextbox.TextChanged += new System.EventHandler(this.widthtextbox_TextChanged);
            this.widthtextbox.Leave += new System.EventHandler(this.widthtextbox_Leave);
            // 
            // label4
            // 
            this.label4.AutoSize = true;
            this.label4.BackColor = System.Drawing.Color.Transparent;
            this.label4.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label4.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.label4.Location = new System.Drawing.Point(10, 10);
            this.label4.Name = "label4";
            this.label4.Size = new System.Drawing.Size(84, 18);
            this.label4.TabIndex = 1033;
            this.label4.Text = "Çözünürlük";
            // 
            // javaTabPage
            // 
            this.javaTabPage.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(64)))), ((int)(((byte)(95)))), ((int)(((byte)(115)))));
            this.javaTabPage.Controls.Add(this.maxRamTextBox);
            this.javaTabPage.Controls.Add(this.minRamTextBox);
            this.javaTabPage.Controls.Add(this.ramInfoLabel);
            this.javaTabPage.Controls.Add(this.jvmTextBox);
            this.javaTabPage.Controls.Add(this.label12);
            this.javaTabPage.Controls.Add(this.maxRamDynamicCalculatorLabel);
            this.javaTabPage.Controls.Add(this.minRamMBtoGBLabel);
            this.javaTabPage.Controls.Add(this.label8);
            this.javaTabPage.Controls.Add(this.label7);
            this.javaTabPage.Controls.Add(this.label6);
            this.javaTabPage.Location = new System.Drawing.Point(4, 34);
            this.javaTabPage.Name = "javaTabPage";
            this.javaTabPage.Size = new System.Drawing.Size(521, 354);
            this.javaTabPage.TabIndex = 4;
            this.javaTabPage.Text = "Java";
            // 
            // maxRamTextBox
            // 
            this.maxRamTextBox.Animated = true;
            this.maxRamTextBox.BackColor = System.Drawing.Color.White;
            this.maxRamTextBox.BorderColor = System.Drawing.Color.WhiteSmoke;
            this.maxRamTextBox.BorderStyle = System.Drawing.Drawing2D.DashStyle.Custom;
            this.maxRamTextBox.Cursor = System.Windows.Forms.Cursors.IBeam;
            this.maxRamTextBox.DefaultText = "99999";
            this.maxRamTextBox.DisabledState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(208)))), ((int)(((byte)(208)))), ((int)(((byte)(208)))));
            this.maxRamTextBox.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(226)))), ((int)(((byte)(226)))), ((int)(((byte)(226)))));
            this.maxRamTextBox.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.maxRamTextBox.DisabledState.PlaceholderForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.maxRamTextBox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(64)))), ((int)(((byte)(95)))), ((int)(((byte)(115)))));
            this.maxRamTextBox.FocusedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.maxRamTextBox.Font = new System.Drawing.Font("Arial", 10F);
            this.maxRamTextBox.ForeColor = System.Drawing.Color.Black;
            this.maxRamTextBox.HoverState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.maxRamTextBox.Location = new System.Drawing.Point(13, 113);
            this.maxRamTextBox.Name = "maxRamTextBox";
            this.maxRamTextBox.PasswordChar = '\0';
            this.maxRamTextBox.PlaceholderText = "";
            this.maxRamTextBox.SelectedText = "";
            this.maxRamTextBox.Size = new System.Drawing.Size(64, 29);
            this.maxRamTextBox.TabIndex = 1048;
            this.maxRamTextBox.WordWrap = false;
            this.maxRamTextBox.TextChanged += new System.EventHandler(this.maxramtext_TextChanged);
            this.maxRamTextBox.Leave += new System.EventHandler(this.maxramtext_Leave);
            // 
            // minRamTextBox
            // 
            this.minRamTextBox.Animated = true;
            this.minRamTextBox.BackColor = System.Drawing.Color.White;
            this.minRamTextBox.BorderColor = System.Drawing.Color.WhiteSmoke;
            this.minRamTextBox.BorderStyle = System.Drawing.Drawing2D.DashStyle.Custom;
            this.minRamTextBox.Cursor = System.Windows.Forms.Cursors.IBeam;
            this.minRamTextBox.DefaultText = "99999";
            this.minRamTextBox.DisabledState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(208)))), ((int)(((byte)(208)))), ((int)(((byte)(208)))));
            this.minRamTextBox.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(226)))), ((int)(((byte)(226)))), ((int)(((byte)(226)))));
            this.minRamTextBox.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.minRamTextBox.DisabledState.PlaceholderForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.minRamTextBox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(64)))), ((int)(((byte)(95)))), ((int)(((byte)(115)))));
            this.minRamTextBox.FocusedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.minRamTextBox.Font = new System.Drawing.Font("Arial", 10F);
            this.minRamTextBox.ForeColor = System.Drawing.Color.Black;
            this.minRamTextBox.HoverState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.minRamTextBox.Location = new System.Drawing.Point(13, 55);
            this.minRamTextBox.Name = "minRamTextBox";
            this.minRamTextBox.PasswordChar = '\0';
            this.minRamTextBox.PlaceholderText = "";
            this.minRamTextBox.SelectedText = "";
            this.minRamTextBox.Size = new System.Drawing.Size(64, 29);
            this.minRamTextBox.TabIndex = 1047;
            this.minRamTextBox.WordWrap = false;
            this.minRamTextBox.TextChanged += new System.EventHandler(this.minramtext_TextChanged);
            this.minRamTextBox.Leave += new System.EventHandler(this.minramtext_Leave);
            // 
            // ramInfoLabel
            // 
            this.ramInfoLabel.AutoSize = true;
            this.ramInfoLabel.BackColor = System.Drawing.Color.Transparent;
            this.ramInfoLabel.Font = new System.Drawing.Font("Arial", 8.25F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.ramInfoLabel.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.ramInfoLabel.Location = new System.Drawing.Point(58, 12);
            this.ramInfoLabel.Name = "ramInfoLabel";
            this.ramInfoLabel.Size = new System.Drawing.Size(131, 14);
            this.ramInfoLabel.TabIndex = 1046;
            this.ramInfoLabel.Text = "(Bulunan RAM: 16000GB)";
            // 
            // jvmTextBox
            // 
            this.jvmTextBox.Animated = true;
            this.jvmTextBox.BackColor = System.Drawing.Color.White;
            this.jvmTextBox.BorderColor = System.Drawing.Color.WhiteSmoke;
            this.jvmTextBox.BorderStyle = System.Drawing.Drawing2D.DashStyle.Custom;
            this.jvmTextBox.Cursor = System.Windows.Forms.Cursors.IBeam;
            this.jvmTextBox.DefaultText = "-XX:+UnlockExperimentalVMOptions XX:+UseG1GC -XX:ParallelGCThreads=8";
            this.jvmTextBox.DisabledState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(208)))), ((int)(((byte)(208)))), ((int)(((byte)(208)))));
            this.jvmTextBox.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(226)))), ((int)(((byte)(226)))), ((int)(((byte)(226)))));
            this.jvmTextBox.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.jvmTextBox.DisabledState.PlaceholderForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.jvmTextBox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(64)))), ((int)(((byte)(95)))), ((int)(((byte)(115)))));
            this.jvmTextBox.FocusedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.jvmTextBox.Font = new System.Drawing.Font("Arial", 9F);
            this.jvmTextBox.ForeColor = System.Drawing.Color.Black;
            this.jvmTextBox.HoverState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.jvmTextBox.Location = new System.Drawing.Point(13, 190);
            this.jvmTextBox.Name = "jvmTextBox";
            this.jvmTextBox.PasswordChar = '\0';
            this.jvmTextBox.PlaceholderText = "";
            this.jvmTextBox.SelectedText = "";
            this.jvmTextBox.Size = new System.Drawing.Size(262, 29);
            this.jvmTextBox.TabIndex = 1045;
            this.jvmTextBox.WordWrap = false;
            this.jvmTextBox.TextChanged += new System.EventHandler(this.jvmTextBox_TextChanged);
            // 
            // label12
            // 
            this.label12.AutoSize = true;
            this.label12.BackColor = System.Drawing.Color.Transparent;
            this.label12.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label12.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.label12.Location = new System.Drawing.Point(10, 169);
            this.label12.Name = "label12";
            this.label12.Size = new System.Drawing.Size(127, 18);
            this.label12.TabIndex = 1044;
            this.label12.Text = "Java Argümanları";
            // 
            // maxRamDynamicCalculatorLabel
            // 
            this.maxRamDynamicCalculatorLabel.AutoSize = true;
            this.maxRamDynamicCalculatorLabel.BackColor = System.Drawing.Color.Transparent;
            this.maxRamDynamicCalculatorLabel.Font = new System.Drawing.Font("Arial", 11.25F, System.Drawing.FontStyle.Bold, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.maxRamDynamicCalculatorLabel.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.maxRamDynamicCalculatorLabel.Location = new System.Drawing.Point(83, 118);
            this.maxRamDynamicCalculatorLabel.Name = "maxRamDynamicCalculatorLabel";
            this.maxRamDynamicCalculatorLabel.Size = new System.Drawing.Size(51, 18);
            this.maxRamDynamicCalculatorLabel.TabIndex = 1043;
            this.maxRamDynamicCalculatorLabel.Text = "15 GB";
            // 
            // minRamMBtoGBLabel
            // 
            this.minRamMBtoGBLabel.AutoSize = true;
            this.minRamMBtoGBLabel.BackColor = System.Drawing.Color.Transparent;
            this.minRamMBtoGBLabel.Font = new System.Drawing.Font("Arial", 11.25F, System.Drawing.FontStyle.Bold, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.minRamMBtoGBLabel.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.minRamMBtoGBLabel.Location = new System.Drawing.Point(83, 60);
            this.minRamMBtoGBLabel.Name = "minRamMBtoGBLabel";
            this.minRamMBtoGBLabel.Size = new System.Drawing.Size(43, 18);
            this.minRamMBtoGBLabel.TabIndex = 1042;
            this.minRamMBtoGBLabel.Text = "2 GB";
            // 
            // label8
            // 
            this.label8.AutoSize = true;
            this.label8.BackColor = System.Drawing.Color.Transparent;
            this.label8.Font = new System.Drawing.Font("Arial", 11.25F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label8.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.label8.Location = new System.Drawing.Point(10, 35);
            this.label8.Name = "label8";
            this.label8.Size = new System.Drawing.Size(67, 17);
            this.label8.TabIndex = 1038;
            this.label8.Text = "Minimum";
            // 
            // label7
            // 
            this.label7.AutoSize = true;
            this.label7.BackColor = System.Drawing.Color.Transparent;
            this.label7.Font = new System.Drawing.Font("Arial", 11.25F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label7.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.label7.Location = new System.Drawing.Point(10, 93);
            this.label7.Name = "label7";
            this.label7.Size = new System.Drawing.Size(71, 17);
            this.label7.TabIndex = 1037;
            this.label7.Text = "Maximum";
            // 
            // label6
            // 
            this.label6.AutoSize = true;
            this.label6.BackColor = System.Drawing.Color.Transparent;
            this.label6.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label6.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.label6.Location = new System.Drawing.Point(10, 10);
            this.label6.Name = "label6";
            this.label6.Size = new System.Drawing.Size(51, 18);
            this.label6.TabIndex = 1035;
            this.label6.Text = "Bellek";
            // 
            // tabPage1
            // 
            this.tabPage1.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(64)))), ((int)(((byte)(95)))), ((int)(((byte)(115)))));
            this.tabPage1.Controls.Add(this.versionLabel);
            this.tabPage1.Controls.Add(this.guna2TextBox4);
            this.tabPage1.Controls.Add(this.label13);
            this.tabPage1.Location = new System.Drawing.Point(4, 34);
            this.tabPage1.Name = "tabPage1";
            this.tabPage1.Size = new System.Drawing.Size(521, 354);
            this.tabPage1.TabIndex = 5;
            this.tabPage1.Text = "Launcher";
            // 
            // versionLabel
            // 
            this.versionLabel.AutoSize = true;
            this.versionLabel.BackColor = System.Drawing.Color.Transparent;
            this.versionLabel.Cursor = System.Windows.Forms.Cursors.Default;
            this.versionLabel.Font = new System.Drawing.Font("Microsoft Sans Serif", 9F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.versionLabel.ForeColor = System.Drawing.SystemColors.Control;
            this.versionLabel.Location = new System.Drawing.Point(477, 336);
            this.versionLabel.Name = "versionLabel";
            this.versionLabel.Size = new System.Drawing.Size(41, 15);
            this.versionLabel.TabIndex = 1048;
            this.versionLabel.Text = global::Projects_Launcher.Properties.Settings.Default.currentVersion;
            // 
            // guna2TextBox4
            // 
            this.guna2TextBox4.Animated = true;
            this.guna2TextBox4.BackColor = System.Drawing.Color.White;
            this.guna2TextBox4.BorderColor = System.Drawing.Color.WhiteSmoke;
            this.guna2TextBox4.BorderStyle = System.Drawing.Drawing2D.DashStyle.Custom;
            this.guna2TextBox4.Cursor = System.Windows.Forms.Cursors.IBeam;
            this.guna2TextBox4.DefaultText = "";
            this.guna2TextBox4.DisabledState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(208)))), ((int)(((byte)(208)))), ((int)(((byte)(208)))));
            this.guna2TextBox4.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(226)))), ((int)(((byte)(226)))), ((int)(((byte)(226)))));
            this.guna2TextBox4.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.guna2TextBox4.DisabledState.PlaceholderForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.guna2TextBox4.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(64)))), ((int)(((byte)(95)))), ((int)(((byte)(115)))));
            this.guna2TextBox4.FocusedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.guna2TextBox4.Font = new System.Drawing.Font("Arial", 9F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.guna2TextBox4.ForeColor = System.Drawing.Color.Black;
            this.guna2TextBox4.HoverState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.guna2TextBox4.Location = new System.Drawing.Point(13, 31);
            this.guna2TextBox4.Name = "guna2TextBox4";
            this.guna2TextBox4.PasswordChar = '\0';
            this.guna2TextBox4.PlaceholderText = "";
            this.guna2TextBox4.SelectedText = "";
            this.guna2TextBox4.Size = new System.Drawing.Size(262, 29);
            this.guna2TextBox4.TabIndex = 1047;
            this.guna2TextBox4.WordWrap = false;
            // 
            // label13
            // 
            this.label13.AutoSize = true;
            this.label13.BackColor = System.Drawing.Color.Transparent;
            this.label13.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label13.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.label13.Location = new System.Drawing.Point(10, 10);
            this.label13.Name = "label13";
            this.label13.Size = new System.Drawing.Size(104, 18);
            this.label13.TabIndex = 1046;
            this.label13.Text = "Oyun Konumu";
            // 
            // maximizeButtonControlBox
            // 
            this.maximizeButtonControlBox.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.maximizeButtonControlBox.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.maximizeButtonControlBox.ControlBoxType = Guna.UI2.WinForms.Enums.ControlBoxType.MaximizeBox;
            this.maximizeButtonControlBox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(25)))), ((int)(((byte)(25)))));
            this.maximizeButtonControlBox.IconColor = System.Drawing.Color.White;
            this.maximizeButtonControlBox.Location = new System.Drawing.Point(949, 0);
            this.maximizeButtonControlBox.Name = "maximizeButtonControlBox";
            this.maximizeButtonControlBox.Size = new System.Drawing.Size(40, 22);
            this.maximizeButtonControlBox.TabIndex = 1035;
            // 
            // label11
            // 
            this.label11.AutoSize = true;
            this.label11.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.label11.ForeColor = System.Drawing.SystemColors.Control;
            this.label11.Location = new System.Drawing.Point(841, 1);
            this.label11.Name = "label11";
            this.label11.Size = new System.Drawing.Size(48, 13);
            this.label11.TabIndex = 1034;
            this.label11.Text = "label10";
            this.label11.Visible = false;
            // 
            // closeButtonControlBox
            // 
            this.closeButtonControlBox.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.closeButtonControlBox.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.closeButtonControlBox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(25)))), ((int)(((byte)(25)))));
            this.closeButtonControlBox.IconColor = System.Drawing.Color.White;
            this.closeButtonControlBox.Location = new System.Drawing.Point(989, 0);
            this.closeButtonControlBox.Name = "closeButtonControlBox";
            this.closeButtonControlBox.Size = new System.Drawing.Size(40, 22);
            this.closeButtonControlBox.TabIndex = 1033;
            this.closeButtonControlBox.Click += new System.EventHandler(this.closeButtonControlBox_Click);
            // 
            // minimizeButtonControlBox
            // 
            this.minimizeButtonControlBox.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.minimizeButtonControlBox.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.minimizeButtonControlBox.ControlBoxType = Guna.UI2.WinForms.Enums.ControlBoxType.MinimizeBox;
            this.minimizeButtonControlBox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(25)))), ((int)(((byte)(25)))));
            this.minimizeButtonControlBox.IconColor = System.Drawing.Color.White;
            this.minimizeButtonControlBox.Location = new System.Drawing.Point(909, 0);
            this.minimizeButtonControlBox.Name = "minimizeButtonControlBox";
            this.minimizeButtonControlBox.Size = new System.Drawing.Size(40, 22);
            this.minimizeButtonControlBox.TabIndex = 1032;
            // 
            // ticksave
            // 
            this.ticksave.AutoSize = true;
            this.ticksave.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.ticksave.ForeColor = System.Drawing.SystemColors.Control;
            this.ticksave.Location = new System.Drawing.Point(785, 1);
            this.ticksave.Name = "ticksave";
            this.ticksave.Size = new System.Drawing.Size(48, 13);
            this.ticksave.TabIndex = 1031;
            this.ticksave.Text = "label10";
            this.ticksave.Visible = false;
            // 
            // minramlabel
            // 
            this.minramlabel.AutoSize = true;
            this.minramlabel.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.minramlabel.ForeColor = System.Drawing.SystemColors.Control;
            this.minramlabel.Location = new System.Drawing.Point(736, 1);
            this.minramlabel.Name = "minramlabel";
            this.minramlabel.Size = new System.Drawing.Size(48, 13);
            this.minramlabel.TabIndex = 1030;
            this.minramlabel.Text = "label10";
            this.minramlabel.Visible = false;
            // 
            // surumtext
            // 
            this.surumtext.AutoSize = true;
            this.surumtext.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.surumtext.ForeColor = System.Drawing.SystemColors.Control;
            this.surumtext.Location = new System.Drawing.Point(673, 1);
            this.surumtext.Name = "surumtext";
            this.surumtext.Size = new System.Drawing.Size(61, 13);
            this.surumtext.TabIndex = 1029;
            this.surumtext.Text = "surumtext";
            this.surumtext.Visible = false;
            // 
            // widthlabel
            // 
            this.widthlabel.AutoSize = true;
            this.widthlabel.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.widthlabel.ForeColor = System.Drawing.SystemColors.Control;
            this.widthlabel.Location = new System.Drawing.Point(550, 1);
            this.widthlabel.Name = "widthlabel";
            this.widthlabel.Size = new System.Drawing.Size(64, 13);
            this.widthlabel.TabIndex = 1028;
            this.widthlabel.Text = "widthlabel";
            this.widthlabel.Visible = false;
            // 
            // heightlabel
            // 
            this.heightlabel.AutoSize = true;
            this.heightlabel.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.heightlabel.ForeColor = System.Drawing.SystemColors.Control;
            this.heightlabel.Location = new System.Drawing.Point(610, 1);
            this.heightlabel.Name = "heightlabel";
            this.heightlabel.Size = new System.Drawing.Size(69, 13);
            this.heightlabel.TabIndex = 1027;
            this.heightlabel.Text = "heightlabel";
            this.heightlabel.Visible = false;
            // 
            // maxramlabel
            // 
            this.maxramlabel.AutoSize = true;
            this.maxramlabel.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.maxramlabel.ForeColor = System.Drawing.SystemColors.Control;
            this.maxramlabel.Location = new System.Drawing.Point(498, 1);
            this.maxramlabel.Name = "maxramlabel";
            this.maxramlabel.Size = new System.Drawing.Size(54, 13);
            this.maxramlabel.TabIndex = 1026;
            this.maxramlabel.Text = "ramlabel";
            this.maxramlabel.Visible = false;
            // 
            // mainMenuForm
            // 
            this.AutoScaleDimensions = new System.Drawing.SizeF(7F, 13F);
            this.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
            this.BackColor = System.Drawing.SystemColors.Control;
            this.BackgroundImage = ((System.Drawing.Image)(resources.GetObject("$this.BackgroundImage")));
            this.BackgroundImageLayout = System.Windows.Forms.ImageLayout.Stretch;
            this.ClientSize = new System.Drawing.Size(1026, 540);
            this.Controls.Add(this.settingsBgPanel);
            this.Controls.Add(this.Panel);
            this.Controls.Add(this.maximizeButtonControlBox);
            this.Controls.Add(this.label11);
            this.Controls.Add(this.closeButtonControlBox);
            this.Controls.Add(this.minimizeButtonControlBox);
            this.Controls.Add(this.ticksave);
            this.Controls.Add(this.minramlabel);
            this.Controls.Add(this.surumtext);
            this.Controls.Add(this.widthlabel);
            this.Controls.Add(this.heightlabel);
            this.Controls.Add(this.maxramlabel);
            this.DoubleBuffered = true;
            this.Font = new System.Drawing.Font("Microsoft Sans Serif", 8.249999F, System.Drawing.FontStyle.Bold, System.Drawing.GraphicsUnit.Point, ((byte)(0)));
            this.FormBorderStyle = System.Windows.Forms.FormBorderStyle.None;
            this.Icon = ((System.Drawing.Icon)(resources.GetObject("$this.Icon")));
            this.Name = "mainMenuForm";
            this.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
            this.Text = "Projects Launcher";
            this.Load += new System.EventHandler(this.Anamenu_Load);
            this.Panel.ResumeLayout(false);
            this.Panel.PerformLayout();
            this.guna2Panel3.ResumeLayout(false);
            this.guna2Panel3.PerformLayout();
            ((System.ComponentModel.ISupportInitialize)(this.skinRenderPictureBox)).EndInit();
            this.guna2Panel4.ResumeLayout(false);
            ((System.ComponentModel.ISupportInitialize)(this.guna2PictureBox10)).EndInit();
            this.guna2Panel5.ResumeLayout(false);
            this.guna2Panel5.PerformLayout();
            this.settingsBgPanel.ResumeLayout(false);
            this.minecraftTabPage.ResumeLayout(false);
            this.minecraftTabPage.PerformLayout();
            this.javaTabPage.ResumeLayout(false);
            this.javaTabPage.PerformLayout();
            this.tabPage1.ResumeLayout(false);
            this.tabPage1.PerformLayout();
            this.ResumeLayout(false);
            this.PerformLayout();

        }

        #endregion
        private System.Windows.Forms.Timer prepareGameToLaunch;
        private System.Windows.Forms.Timer timer3;
        private Guna.UI2.WinForms.Guna2BorderlessForm guna2BorderlessForm1;
        private System.Windows.Forms.Timer serverPing;
        private System.DirectoryServices.DirectorySearcher directorySearcher1;
        private Guna.UI2.WinForms.Guna2Panel Panel;
        private Guna.UI2.WinForms.Guna2TabControl settingsBgPanel;
        private System.Windows.Forms.TabPage minecraftTabPage;
        private Guna.UI2.WinForms.Guna2CheckBox reopenLauncher;
        private Guna.UI2.WinForms.Guna2CheckBox autoConnect;
        private Guna.UI2.WinForms.Guna2CheckBox fullscreenCheckBox;
        private Guna.UI2.WinForms.Guna2TextBox heighttextbox;
        private System.Windows.Forms.Label label5;
        private Guna.UI2.WinForms.Guna2TextBox widthtextbox;
        private System.Windows.Forms.Label label4;
        private System.Windows.Forms.TabPage javaTabPage;
        private Guna.UI2.WinForms.Guna2TextBox jvmTextBox;
        private System.Windows.Forms.Label label12;
        private System.Windows.Forms.Label maxRamDynamicCalculatorLabel;
        private System.Windows.Forms.Label minRamMBtoGBLabel;
        private System.Windows.Forms.Label label8;
        private System.Windows.Forms.Label label7;
        private System.Windows.Forms.Label label6;
        private System.Windows.Forms.TabPage tabPage1;
        private Guna.UI2.WinForms.Guna2TextBox guna2TextBox4;
        private System.Windows.Forms.Label label13;
        private Guna.UI2.WinForms.Guna2Panel guna2Panel3;
        private System.Windows.Forms.Label playerNameStaticLabell;
        private Guna.UI2.WinForms.Guna2ImageButton settingsStaticPictureBox;
        private Guna.UI2.WinForms.Guna2CirclePictureBox skinRenderPictureBox;
        private System.Windows.Forms.Label playerNameStaticLabel;
        private Guna.UI2.WinForms.Guna2Panel guna2Panel4;
        private Guna.UI2.WinForms.Guna2PictureBox guna2PictureBox10;
        private Guna.UI2.WinForms.Guna2ImageButton instagramStaticPictureBox;
        private Guna.UI2.WinForms.Guna2ImageButton discordStaticPictureBox;
        private Guna.UI2.WinForms.Guna2ImageButton mailStaticPictureBox;
        private Guna.UI2.WinForms.Guna2ImageButton websiteStaticPictureBox;
        private Guna.UI2.WinForms.Guna2Panel guna2Panel5;
        private System.Windows.Forms.Label lobiOnline;
        private System.Windows.Forms.Label label3;
        private System.Windows.Forms.Label serverOnlineCountStaticLabel;
        private System.Windows.Forms.Label label2;
        private Guna.UI2.WinForms.Guna2Button playButtonStaticLabel;
        private System.Windows.Forms.Label versionInfoStaticLabel;
        private Guna.UI2.WinForms.Guna2ProgressBar downloadCompleteBar;
        private System.Windows.Forms.Label playSplitStaticLabel;
        private Guna.UI2.WinForms.Guna2ControlBox maximizeButtonControlBox;
        public System.Windows.Forms.Label label11;
        private Guna.UI2.WinForms.Guna2ControlBox closeButtonControlBox;
        private Guna.UI2.WinForms.Guna2ControlBox minimizeButtonControlBox;
        public System.Windows.Forms.Label ticksave;
        public System.Windows.Forms.Label minramlabel;
        public System.Windows.Forms.Label surumtext;
        public System.Windows.Forms.Label widthlabel;
        public System.Windows.Forms.Label heightlabel;
        public System.Windows.Forms.Label maxramlabel;
        private System.Windows.Forms.Label label9;
        public Guna.UI2.WinForms.Guna2ComboBox versionBox;
        private System.Windows.Forms.Label ramInfoLabel;
        private System.Windows.Forms.Label versionLabel;
        private Guna.UI2.WinForms.Guna2TextBox maxRamTextBox;
        private Guna.UI2.WinForms.Guna2TextBox minRamTextBox;
        private Guna.UI2.WinForms.Guna2ProgressBar PLauncherFC;
    }
}