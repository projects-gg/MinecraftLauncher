﻿
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
            this.serverIpAddressStaticLabel = new System.Windows.Forms.Label();
            this.loginButton = new Guna.UI2.WinForms.Guna2Button();
            this.nickNameEnterTextBox = new Guna.UI2.WinForms.Guna2TextBox();
            this.maximizeButtonControlBox = new Guna.UI2.WinForms.Guna2ControlBox();
            this.minimizeButtonControlBox = new Guna.UI2.WinForms.Guna2ControlBox();
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
            this.rememberMeCheckBox.Anchor = System.Windows.Forms.AnchorStyles.Top;
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
            // serverIpAddressStaticLabel
            // 
            this.serverIpAddressStaticLabel.Anchor = System.Windows.Forms.AnchorStyles.Top;
            this.serverIpAddressStaticLabel.AutoSize = true;
            this.serverIpAddressStaticLabel.BackColor = System.Drawing.Color.Transparent;
            this.serverIpAddressStaticLabel.Cursor = System.Windows.Forms.Cursors.Hand;
            this.serverIpAddressStaticLabel.Font = new System.Drawing.Font("Segoe UI", 12F);
            this.serverIpAddressStaticLabel.ForeColor = System.Drawing.Color.White;
            this.serverIpAddressStaticLabel.Location = new System.Drawing.Point(435, 284);
            this.serverIpAddressStaticLabel.Name = "serverIpAddressStaticLabel";
            this.serverIpAddressStaticLabel.Size = new System.Drawing.Size(110, 21);
            this.serverIpAddressStaticLabel.TabIndex = 29;
            this.serverIpAddressStaticLabel.Text = "mc.projects.gg";
            this.serverIpAddressStaticLabel.Click += new System.EventHandler(this.label3_Click);
            // 
            // loginButton
            // 
            this.loginButton.Anchor = System.Windows.Forms.AnchorStyles.Top;
            this.loginButton.Animated = true;
            this.loginButton.BackColor = System.Drawing.Color.Transparent;
            this.loginButton.BorderRadius = 5;
            this.loginButton.BorderThickness = 1;
            this.loginButton.DisabledState.BorderColor = System.Drawing.Color.DarkGray;
            this.loginButton.DisabledState.CustomBorderColor = System.Drawing.Color.DarkGray;
            this.loginButton.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(169)))), ((int)(((byte)(169)))), ((int)(((byte)(169)))));
            this.loginButton.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(141)))), ((int)(((byte)(141)))), ((int)(((byte)(141)))));
            this.loginButton.FillColor = System.Drawing.Color.Transparent;
            this.loginButton.Font = new System.Drawing.Font("Segoe UI", 12F);
            this.loginButton.ForeColor = System.Drawing.Color.White;
            this.loginButton.Location = new System.Drawing.Point(405, 248);
            this.loginButton.Name = "loginButton";
            this.loginButton.Size = new System.Drawing.Size(170, 33);
            this.loginButton.TabIndex = 27;
            this.loginButton.Text = "Kullanıcı Adı Giriniz";
            this.loginButton.Click += new System.EventHandler(this.girisyapbutton_Click);
            // 
            // nickNameEnterTextBox
            // 
            this.nickNameEnterTextBox.Anchor = System.Windows.Forms.AnchorStyles.Top;
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
            this.nickNameEnterTextBox.IconLeft = global::Projects_Launcher.Properties.Resources.projects;
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
            // maximizeButtonControlBox
            // 
            this.maximizeButtonControlBox.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.maximizeButtonControlBox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.maximizeButtonControlBox.IconColor = System.Drawing.Color.White;
            this.maximizeButtonControlBox.Location = new System.Drawing.Point(935, 0);
            this.maximizeButtonControlBox.Name = "maximizeButtonControlBox";
            this.maximizeButtonControlBox.Size = new System.Drawing.Size(45, 29);
            this.maximizeButtonControlBox.TabIndex = 25;
            this.maximizeButtonControlBox.Click += new System.EventHandler(this.guna2ControlBox1_Click);
            // 
            // minimizeButtonControlBox
            // 
            this.minimizeButtonControlBox.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.minimizeButtonControlBox.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.minimizeButtonControlBox.ControlBoxType = Guna.UI2.WinForms.Enums.ControlBoxType.MinimizeBox;
            this.minimizeButtonControlBox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.minimizeButtonControlBox.IconColor = System.Drawing.Color.White;
            this.minimizeButtonControlBox.Location = new System.Drawing.Point(890, 0);
            this.minimizeButtonControlBox.Name = "minimizeButtonControlBox";
            this.minimizeButtonControlBox.Size = new System.Drawing.Size(45, 29);
            this.minimizeButtonControlBox.TabIndex = 136;
            this.minimizeButtonControlBox.Click += new System.EventHandler(this.guna2ControlBox2_Click);
            // 
            // loginMenuForm
            // 
            this.AutoScaleDimensions = new System.Drawing.SizeF(6F, 13F);
            this.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
            this.AutoSize = true;
            this.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.BackgroundImage = ((System.Drawing.Image)(resources.GetObject("$this.BackgroundImage")));
            this.BackgroundImageLayout = System.Windows.Forms.ImageLayout.Stretch;
            this.ClientSize = new System.Drawing.Size(980, 503);
            this.Controls.Add(this.minimizeButtonControlBox);
            this.Controls.Add(this.raminfo);
            this.Controls.Add(this.rememberMeCheckBox);
            this.Controls.Add(this.serverIpAddressStaticLabel);
            this.Controls.Add(this.loginButton);
            this.Controls.Add(this.nickNameEnterTextBox);
            this.Controls.Add(this.maximizeButtonControlBox);
            this.DoubleBuffered = true;
            this.FormBorderStyle = System.Windows.Forms.FormBorderStyle.None;
            this.Icon = ((System.Drawing.Icon)(resources.GetObject("$this.Icon")));
            this.MaximizeBox = false;
            this.MaximumSize = new System.Drawing.Size(980, 503);
            this.MinimumSize = new System.Drawing.Size(980, 503);
            this.Name = "loginMenuForm";
            this.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
            this.Text = "Projects Launcher";
            this.Load += new System.EventHandler(this.ProjectsLauncherLogin_Load);
            this.ResumeLayout(false);
            this.PerformLayout();

        }

        #endregion
        private Guna.UI2.WinForms.Guna2BorderlessForm guna2BorderlessForm1;
        private System.Windows.Forms.Label raminfo;
        private Guna.UI2.WinForms.Guna2CheckBox rememberMeCheckBox;
        private System.Windows.Forms.Label serverIpAddressStaticLabel;
        private Guna.UI2.WinForms.Guna2Button loginButton;
        private Guna.UI2.WinForms.Guna2TextBox nickNameEnterTextBox;
        private Guna.UI2.WinForms.Guna2ControlBox maximizeButtonControlBox;
        private Guna.UI2.WinForms.Guna2ControlBox minimizeButtonControlBox;
    }
}

