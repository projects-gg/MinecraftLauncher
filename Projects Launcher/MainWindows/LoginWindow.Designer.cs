
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
            System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(loginMenuForm));
            this.webbutton = new System.Windows.Forms.Label();
            this.loginButton = new Guna.UI2.WinForms.Guna2GradientButton();
            this.rememberMeCheckBox = new Guna.UI2.WinForms.Guna2CheckBox();
            this.nickNameEnterTextBox = new Guna.UI2.WinForms.Guna2TextBox();
            this.skinRenderPictureBox = new Guna.UI2.WinForms.Guna2PictureBox();
            ((System.ComponentModel.ISupportInitialize)(this.skinRenderPictureBox)).BeginInit();
            this.SuspendLayout();
            // 
            // webbutton
            // 
            this.webbutton.AutoSize = true;
            this.webbutton.BackColor = System.Drawing.Color.Transparent;
            this.webbutton.Cursor = System.Windows.Forms.Cursors.Hand;
            this.webbutton.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Underline, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.webbutton.ForeColor = System.Drawing.Color.White;
            this.webbutton.Location = new System.Drawing.Point(575, 655);
            this.webbutton.Name = "webbutton";
            this.webbutton.Size = new System.Drawing.Size(110, 18);
            this.webbutton.TabIndex = 1023;
            this.webbutton.Text = "mc.projects.gg";
            this.webbutton.Click += new System.EventHandler(this.webbutton_Click);
            // 
            // loginButton
            // 
            this.loginButton.BackColor = System.Drawing.Color.Transparent;
            this.loginButton.BorderRadius = 12;
            this.loginButton.Cursor = System.Windows.Forms.Cursors.Hand;
            this.loginButton.DisabledState.BorderColor = System.Drawing.Color.DarkGray;
            this.loginButton.DisabledState.CustomBorderColor = System.Drawing.Color.DarkGray;
            this.loginButton.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(169)))), ((int)(((byte)(169)))), ((int)(((byte)(169)))));
            this.loginButton.DisabledState.FillColor2 = System.Drawing.Color.FromArgb(((int)(((byte)(169)))), ((int)(((byte)(169)))), ((int)(((byte)(169)))));
            this.loginButton.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(141)))), ((int)(((byte)(141)))), ((int)(((byte)(141)))));
            this.loginButton.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(140)))), ((int)(((byte)(97)))), ((int)(((byte)(0)))));
            this.loginButton.FillColor2 = System.Drawing.Color.FromArgb(((int)(((byte)(255)))), ((int)(((byte)(97)))), ((int)(((byte)(0)))));
            this.loginButton.Font = new System.Drawing.Font("Arial", 12F);
            this.loginButton.ForeColor = System.Drawing.Color.White;
            this.loginButton.Location = new System.Drawing.Point(540, 390);
            this.loginButton.Name = "loginButton";
            this.loginButton.Size = new System.Drawing.Size(200, 30);
            this.loginButton.TabIndex = 1022;
            this.loginButton.Text = "Giriş Yap";
            this.loginButton.Click += new System.EventHandler(this.loginButton_Click);
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
            this.rememberMeCheckBox.Cursor = System.Windows.Forms.Cursors.Hand;
            this.rememberMeCheckBox.Font = new System.Drawing.Font("Microsoft Sans Serif", 10F);
            this.rememberMeCheckBox.ForeColor = System.Drawing.Color.White;
            this.rememberMeCheckBox.Location = new System.Drawing.Point(540, 337);
            this.rememberMeCheckBox.Name = "rememberMeCheckBox";
            this.rememberMeCheckBox.Size = new System.Drawing.Size(100, 21);
            this.rememberMeCheckBox.TabIndex = 1021;
            this.rememberMeCheckBox.Text = "Beni Hatırla";
            this.rememberMeCheckBox.UncheckedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(125)))), ((int)(((byte)(137)))), ((int)(((byte)(149)))));
            this.rememberMeCheckBox.UncheckedState.BorderRadius = 0;
            this.rememberMeCheckBox.UncheckedState.BorderThickness = 0;
            this.rememberMeCheckBox.UncheckedState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(125)))), ((int)(((byte)(137)))), ((int)(((byte)(149)))));
            this.rememberMeCheckBox.UseVisualStyleBackColor = false;
            this.rememberMeCheckBox.CheckedChanged += new System.EventHandler(this.rememberMeCheckBox_CheckedChanged);
            // 
            // nickNameEnterTextBox
            // 
            this.nickNameEnterTextBox.BackColor = System.Drawing.Color.BlueViolet;
            this.nickNameEnterTextBox.Cursor = System.Windows.Forms.Cursors.IBeam;
            this.nickNameEnterTextBox.DefaultText = "";
            this.nickNameEnterTextBox.DisabledState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(208)))), ((int)(((byte)(208)))), ((int)(((byte)(208)))));
            this.nickNameEnterTextBox.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(226)))), ((int)(((byte)(226)))), ((int)(((byte)(226)))));
            this.nickNameEnterTextBox.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.nickNameEnterTextBox.DisabledState.PlaceholderForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.nickNameEnterTextBox.FocusedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.nickNameEnterTextBox.Font = new System.Drawing.Font("Segoe UI", 12F);
            this.nickNameEnterTextBox.ForeColor = System.Drawing.Color.Black;
            this.nickNameEnterTextBox.HoverState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.nickNameEnterTextBox.Location = new System.Drawing.Point(540, 294);
            this.nickNameEnterTextBox.Margin = new System.Windows.Forms.Padding(4, 4, 4, 4);
            this.nickNameEnterTextBox.Name = "nickNameEnterTextBox";
            this.nickNameEnterTextBox.PasswordChar = '\0';
            this.nickNameEnterTextBox.PlaceholderText = "";
            this.nickNameEnterTextBox.SelectedText = "";
            this.nickNameEnterTextBox.Size = new System.Drawing.Size(200, 36);
            this.nickNameEnterTextBox.TabIndex = 1020;
            this.nickNameEnterTextBox.TextChanged += new System.EventHandler(this.nickNameEnterTextBox_TextChanged);
            // 
            // skinRenderPictureBox
            // 
            this.skinRenderPictureBox.BackColor = System.Drawing.Color.Transparent;
            this.skinRenderPictureBox.Image = ((System.Drawing.Image)(resources.GetObject("skinRenderPictureBox.Image")));
            this.skinRenderPictureBox.ImageRotate = 0F;
            this.skinRenderPictureBox.Location = new System.Drawing.Point(595, 143);
            this.skinRenderPictureBox.Name = "skinRenderPictureBox";
            this.skinRenderPictureBox.Size = new System.Drawing.Size(90, 90);
            this.skinRenderPictureBox.SizeMode = System.Windows.Forms.PictureBoxSizeMode.StretchImage;
            this.skinRenderPictureBox.TabIndex = 1019;
            this.skinRenderPictureBox.TabStop = false;
            // 
            // loginMenuForm
            // 
            this.AutoScaleDimensions = new System.Drawing.SizeF(6F, 13F);
            this.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
            this.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.BackgroundImage = global::Projects_Launcher.Properties.Resources.Background;
            this.BackgroundImageLayout = System.Windows.Forms.ImageLayout.Stretch;
            this.ClientSize = new System.Drawing.Size(1264, 681);
            this.Controls.Add(this.webbutton);
            this.Controls.Add(this.loginButton);
            this.Controls.Add(this.rememberMeCheckBox);
            this.Controls.Add(this.nickNameEnterTextBox);
            this.Controls.Add(this.skinRenderPictureBox);
            this.DoubleBuffered = true;
            this.Icon = ((System.Drawing.Icon)(resources.GetObject("$this.Icon")));
            this.MaximumSize = new System.Drawing.Size(1280, 720);
            this.MinimumSize = new System.Drawing.Size(640, 360);
            this.Name = "loginMenuForm";
            this.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
            this.Text = "Projects Launcher";
            this.FormClosed += new System.Windows.Forms.FormClosedEventHandler(this.loginMenuForm_FormClosed);
            this.Load += new System.EventHandler(this.ProjectsLauncherLogin_Load);
            ((System.ComponentModel.ISupportInitialize)(this.skinRenderPictureBox)).EndInit();
            this.ResumeLayout(false);
            this.PerformLayout();

        }

        #endregion

        private System.Windows.Forms.Label webbutton;
        private Guna.UI2.WinForms.Guna2GradientButton loginButton;
        private Guna.UI2.WinForms.Guna2CheckBox rememberMeCheckBox;
        private Guna.UI2.WinForms.Guna2TextBox nickNameEnterTextBox;
        private Guna.UI2.WinForms.Guna2PictureBox skinRenderPictureBox;
    }
}

