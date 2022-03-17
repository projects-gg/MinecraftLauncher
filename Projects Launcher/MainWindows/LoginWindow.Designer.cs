
namespace Projects_Launcher
{
    partial class ProjectsLauncherLogin
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
            System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(ProjectsLauncherLogin));
            this.guna2BorderlessForm1 = new Guna.UI2.WinForms.Guna2BorderlessForm(this.components);
            this.raminfo = new System.Windows.Forms.Label();
            this.benihatırla = new Guna.UI2.WinForms.Guna2CheckBox();
            this.projectslabel = new System.Windows.Forms.Label();
            this.girisyapbutton = new Guna.UI2.WinForms.Guna2Button();
            this.nicknametextbox = new Guna.UI2.WinForms.Guna2TextBox();
            this.guna2ControlBox1 = new Guna.UI2.WinForms.Guna2ControlBox();
            this.guna2ControlBox2 = new Guna.UI2.WinForms.Guna2ControlBox();
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
            // benihatırla
            // 
            this.benihatırla.Anchor = System.Windows.Forms.AnchorStyles.Top;
            this.benihatırla.Animated = true;
            this.benihatırla.AutoCheck = false;
            this.benihatırla.AutoSize = true;
            this.benihatırla.BackColor = System.Drawing.Color.Transparent;
            this.benihatırla.Checked = true;
            this.benihatırla.CheckedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(248)))), ((int)(((byte)(148)))), ((int)(((byte)(35)))));
            this.benihatırla.CheckedState.BorderRadius = 0;
            this.benihatırla.CheckedState.BorderThickness = 0;
            this.benihatırla.CheckedState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(248)))), ((int)(((byte)(148)))), ((int)(((byte)(35)))));
            this.benihatırla.CheckState = System.Windows.Forms.CheckState.Checked;
            this.benihatırla.Font = new System.Drawing.Font("Microsoft Sans Serif", 12F);
            this.benihatırla.ForeColor = System.Drawing.SystemColors.ControlLightLight;
            this.benihatırla.Location = new System.Drawing.Point(380, 205);
            this.benihatırla.Name = "benihatırla";
            this.benihatırla.Size = new System.Drawing.Size(110, 24);
            this.benihatırla.TabIndex = 32;
            this.benihatırla.Text = "Beni Hatırla";
            this.benihatırla.UncheckedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(125)))), ((int)(((byte)(137)))), ((int)(((byte)(149)))));
            this.benihatırla.UncheckedState.BorderRadius = 0;
            this.benihatırla.UncheckedState.BorderThickness = 0;
            this.benihatırla.UncheckedState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(125)))), ((int)(((byte)(137)))), ((int)(((byte)(149)))));
            this.benihatırla.UseVisualStyleBackColor = false;
            this.benihatırla.CheckedChanged += new System.EventHandler(this.benihatırla_CheckedChanged);
            // 
            // projectslabel
            // 
            this.projectslabel.Anchor = System.Windows.Forms.AnchorStyles.Top;
            this.projectslabel.AutoSize = true;
            this.projectslabel.BackColor = System.Drawing.Color.Transparent;
            this.projectslabel.Cursor = System.Windows.Forms.Cursors.Hand;
            this.projectslabel.Font = new System.Drawing.Font("Segoe UI", 12F);
            this.projectslabel.ForeColor = System.Drawing.Color.White;
            this.projectslabel.Location = new System.Drawing.Point(435, 284);
            this.projectslabel.Name = "projectslabel";
            this.projectslabel.Size = new System.Drawing.Size(110, 21);
            this.projectslabel.TabIndex = 29;
            this.projectslabel.Text = "mc.projects.gg";
            this.projectslabel.Click += new System.EventHandler(this.label3_Click);
            // 
            // girisyapbutton
            // 
            this.girisyapbutton.Anchor = System.Windows.Forms.AnchorStyles.Top;
            this.girisyapbutton.Animated = true;
            this.girisyapbutton.BackColor = System.Drawing.Color.Transparent;
            this.girisyapbutton.BorderRadius = 5;
            this.girisyapbutton.BorderThickness = 1;
            this.girisyapbutton.DisabledState.BorderColor = System.Drawing.Color.DarkGray;
            this.girisyapbutton.DisabledState.CustomBorderColor = System.Drawing.Color.DarkGray;
            this.girisyapbutton.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(169)))), ((int)(((byte)(169)))), ((int)(((byte)(169)))));
            this.girisyapbutton.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(141)))), ((int)(((byte)(141)))), ((int)(((byte)(141)))));
            this.girisyapbutton.FillColor = System.Drawing.Color.Transparent;
            this.girisyapbutton.Font = new System.Drawing.Font("Segoe UI", 12F);
            this.girisyapbutton.ForeColor = System.Drawing.Color.White;
            this.girisyapbutton.Location = new System.Drawing.Point(405, 248);
            this.girisyapbutton.Name = "girisyapbutton";
            this.girisyapbutton.Size = new System.Drawing.Size(170, 33);
            this.girisyapbutton.TabIndex = 27;
            this.girisyapbutton.Text = "Kullanıcı Adı Giriniz";
            this.girisyapbutton.Click += new System.EventHandler(this.girisyapbutton_Click);
            // 
            // nicknametextbox
            // 
            this.nicknametextbox.Anchor = System.Windows.Forms.AnchorStyles.Top;
            this.nicknametextbox.BackColor = System.Drawing.Color.Tomato;
            this.nicknametextbox.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(64)))), ((int)(((byte)(64)))), ((int)(((byte)(64)))));
            this.nicknametextbox.Cursor = System.Windows.Forms.Cursors.IBeam;
            this.nicknametextbox.DefaultText = "Kullanıcı Adı";
            this.nicknametextbox.DisabledState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(208)))), ((int)(((byte)(208)))), ((int)(((byte)(208)))));
            this.nicknametextbox.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(226)))), ((int)(((byte)(226)))), ((int)(((byte)(226)))));
            this.nicknametextbox.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.nicknametextbox.DisabledState.PlaceholderForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.nicknametextbox.FillColor = System.Drawing.Color.Transparent;
            this.nicknametextbox.FocusedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.nicknametextbox.Font = new System.Drawing.Font("Segoe UI", 14.25F);
            this.nicknametextbox.ForeColor = System.Drawing.Color.White;
            this.nicknametextbox.HoverState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.nicknametextbox.IconLeft = global::Projects_Launcher.Properties.Resources.projects;
            this.nicknametextbox.Location = new System.Drawing.Point(380, 171);
            this.nicknametextbox.Margin = new System.Windows.Forms.Padding(4);
            this.nicknametextbox.Name = "nicknametextbox";
            this.nicknametextbox.PasswordChar = '\0';
            this.nicknametextbox.PlaceholderForeColor = System.Drawing.Color.Black;
            this.nicknametextbox.PlaceholderText = "";
            this.nicknametextbox.SelectedText = "";
            this.nicknametextbox.Size = new System.Drawing.Size(220, 34);
            this.nicknametextbox.TabIndex = 26;
            this.nicknametextbox.TextChanged += new System.EventHandler(this.nicknametextbox_TextChanged);
            // 
            // guna2ControlBox1
            // 
            this.guna2ControlBox1.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.guna2ControlBox1.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.guna2ControlBox1.IconColor = System.Drawing.Color.White;
            this.guna2ControlBox1.Location = new System.Drawing.Point(935, 0);
            this.guna2ControlBox1.Name = "guna2ControlBox1";
            this.guna2ControlBox1.Size = new System.Drawing.Size(45, 29);
            this.guna2ControlBox1.TabIndex = 25;
            this.guna2ControlBox1.Click += new System.EventHandler(this.guna2ControlBox1_Click);
            // 
            // guna2ControlBox2
            // 
            this.guna2ControlBox2.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.guna2ControlBox2.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.guna2ControlBox2.ControlBoxType = Guna.UI2.WinForms.Enums.ControlBoxType.MinimizeBox;
            this.guna2ControlBox2.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.guna2ControlBox2.IconColor = System.Drawing.Color.White;
            this.guna2ControlBox2.Location = new System.Drawing.Point(890, 0);
            this.guna2ControlBox2.Name = "guna2ControlBox2";
            this.guna2ControlBox2.Size = new System.Drawing.Size(45, 29);
            this.guna2ControlBox2.TabIndex = 136;
            this.guna2ControlBox2.Click += new System.EventHandler(this.guna2ControlBox2_Click);
            // 
            // ProjectsLauncherLogin
            // 
            this.AutoScaleDimensions = new System.Drawing.SizeF(6F, 13F);
            this.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
            this.AutoSize = true;
            this.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.BackgroundImage = ((System.Drawing.Image)(resources.GetObject("$this.BackgroundImage")));
            this.BackgroundImageLayout = System.Windows.Forms.ImageLayout.Stretch;
            this.ClientSize = new System.Drawing.Size(980, 503);
            this.Controls.Add(this.guna2ControlBox2);
            this.Controls.Add(this.raminfo);
            this.Controls.Add(this.benihatırla);
            this.Controls.Add(this.projectslabel);
            this.Controls.Add(this.girisyapbutton);
            this.Controls.Add(this.nicknametextbox);
            this.Controls.Add(this.guna2ControlBox1);
            this.DoubleBuffered = true;
            this.FormBorderStyle = System.Windows.Forms.FormBorderStyle.None;
            this.Icon = ((System.Drawing.Icon)(resources.GetObject("$this.Icon")));
            this.MaximizeBox = false;
            this.MaximumSize = new System.Drawing.Size(980, 503);
            this.MinimumSize = new System.Drawing.Size(980, 503);
            this.Name = "ProjectsLauncherLogin";
            this.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
            this.Text = "Projects Launcher";
            this.Load += new System.EventHandler(this.ProjectsLauncherLogin_Load);
            this.ResumeLayout(false);
            this.PerformLayout();

        }

        #endregion
        private Guna.UI2.WinForms.Guna2BorderlessForm guna2BorderlessForm1;
        private System.Windows.Forms.Label raminfo;
        private Guna.UI2.WinForms.Guna2CheckBox benihatırla;
        private System.Windows.Forms.Label projectslabel;
        private Guna.UI2.WinForms.Guna2Button girisyapbutton;
        private Guna.UI2.WinForms.Guna2TextBox nicknametextbox;
        private Guna.UI2.WinForms.Guna2ControlBox guna2ControlBox1;
        private Guna.UI2.WinForms.Guna2ControlBox guna2ControlBox2;
    }
}

