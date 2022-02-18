
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
            this.label6 = new System.Windows.Forms.Label();
            this.raminfo = new System.Windows.Forms.Label();
            this.benihatırla = new Guna.UI2.WinForms.Guna2CheckBox();
            this.label4 = new System.Windows.Forms.Label();
            this.label5 = new System.Windows.Forms.Label();
            this.label2 = new System.Windows.Forms.Label();
            this.label3 = new System.Windows.Forms.Label();
            this.girisyapbutton = new Guna.UI2.WinForms.Guna2Button();
            this.nicknametextbox = new Guna.UI2.WinForms.Guna2TextBox();
            this.guna2ControlBox1 = new Guna.UI2.WinForms.Guna2ControlBox();
            this.pictureBox1 = new System.Windows.Forms.PictureBox();
            ((System.ComponentModel.ISupportInitialize)(this.pictureBox1)).BeginInit();
            this.SuspendLayout();
            // 
            // guna2BorderlessForm1
            // 
            this.guna2BorderlessForm1.ContainerControl = this;
            this.guna2BorderlessForm1.DockIndicatorTransparencyValue = 0.6D;
            this.guna2BorderlessForm1.TransparentWhileDrag = true;
            // 
            // label6
            // 
            this.label6.AutoSize = true;
            this.label6.BackColor = System.Drawing.Color.Transparent;
            this.label6.Cursor = System.Windows.Forms.Cursors.Hand;
            this.label6.Font = new System.Drawing.Font("Arial", 15.75F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label6.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.label6.Location = new System.Drawing.Point(862, 818);
            this.label6.Name = "label6";
            this.label6.Size = new System.Drawing.Size(58, 24);
            this.label6.TabIndex = 34;
            this.label6.Text = "4096";
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
            this.benihatırla.Location = new System.Drawing.Point(640, 434);
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
            // label4
            // 
            this.label4.AutoSize = true;
            this.label4.BackColor = System.Drawing.Color.Transparent;
            this.label4.Font = new System.Drawing.Font("Arial", 15.75F, System.Drawing.FontStyle.Bold, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label4.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(130)))), ((int)(((byte)(30)))), ((int)(((byte)(240)))));
            this.label4.Location = new System.Drawing.Point(613, 671);
            this.label4.Name = "label4";
            this.label4.Size = new System.Drawing.Size(136, 24);
            this.label4.TabIndex = 30;
            this.label4.Text = "Sunucu URL";
            // 
            // label5
            // 
            this.label5.AutoSize = true;
            this.label5.BackColor = System.Drawing.Color.Transparent;
            this.label5.Cursor = System.Windows.Forms.Cursors.Hand;
            this.label5.Font = new System.Drawing.Font("Arial", 15.75F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label5.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(130)))), ((int)(((byte)(30)))), ((int)(((byte)(240)))));
            this.label5.Location = new System.Drawing.Point(766, 671);
            this.label5.Name = "label5";
            this.label5.Size = new System.Drawing.Size(162, 24);
            this.label5.TabIndex = 31;
            this.label5.Text = "www.projects.gg";
            // 
            // label2
            // 
            this.label2.AutoSize = true;
            this.label2.BackColor = System.Drawing.Color.Transparent;
            this.label2.Font = new System.Drawing.Font("Arial", 15.75F, System.Drawing.FontStyle.Bold, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label2.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(130)))), ((int)(((byte)(30)))), ((int)(((byte)(240)))));
            this.label2.Location = new System.Drawing.Point(636, 634);
            this.label2.Name = "label2";
            this.label2.Size = new System.Drawing.Size(113, 24);
            this.label2.TabIndex = 28;
            this.label2.Text = "Sunucu IP";
            // 
            // label3
            // 
            this.label3.AutoSize = true;
            this.label3.BackColor = System.Drawing.Color.Transparent;
            this.label3.Cursor = System.Windows.Forms.Cursors.Hand;
            this.label3.Font = new System.Drawing.Font("Arial", 15.75F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.label3.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(130)))), ((int)(((byte)(30)))), ((int)(((byte)(240)))));
            this.label3.Location = new System.Drawing.Point(766, 634);
            this.label3.Name = "label3";
            this.label3.Size = new System.Drawing.Size(145, 24);
            this.label3.TabIndex = 29;
            this.label3.Text = "mc.projects.gg";
            this.label3.Click += new System.EventHandler(this.label3_Click);
            // 
            // girisyapbutton
            // 
            this.girisyapbutton.Animated = true;
            this.girisyapbutton.BackColor = System.Drawing.Color.Transparent;
            this.girisyapbutton.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(64)))), ((int)(((byte)(64)))), ((int)(((byte)(64)))));
            this.girisyapbutton.DisabledState.BorderColor = System.Drawing.Color.DarkGray;
            this.girisyapbutton.DisabledState.CustomBorderColor = System.Drawing.Color.DarkGray;
            this.girisyapbutton.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(169)))), ((int)(((byte)(169)))), ((int)(((byte)(169)))));
            this.girisyapbutton.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(141)))), ((int)(((byte)(141)))), ((int)(((byte)(141)))));
            this.girisyapbutton.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(130)))), ((int)(((byte)(30)))), ((int)(((byte)(240)))));
            this.girisyapbutton.Font = new System.Drawing.Font("Segoe UI", 12F);
            this.girisyapbutton.ForeColor = System.Drawing.Color.White;
            this.girisyapbutton.Location = new System.Drawing.Point(674, 500);
            this.girisyapbutton.Name = "girisyapbutton";
            this.girisyapbutton.Size = new System.Drawing.Size(186, 54);
            this.girisyapbutton.TabIndex = 27;
            this.girisyapbutton.Text = "Bekleniyor";
            this.girisyapbutton.Click += new System.EventHandler(this.girisyapbutton_Click);
            // 
            // nicknametextbox
            // 
            this.nicknametextbox.BackColor = System.Drawing.Color.Transparent;
            this.nicknametextbox.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(64)))), ((int)(((byte)(64)))), ((int)(((byte)(64)))));
            this.nicknametextbox.Cursor = System.Windows.Forms.Cursors.IBeam;
            this.nicknametextbox.DefaultText = "";
            this.nicknametextbox.DisabledState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(208)))), ((int)(((byte)(208)))), ((int)(((byte)(208)))));
            this.nicknametextbox.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(226)))), ((int)(((byte)(226)))), ((int)(((byte)(226)))));
            this.nicknametextbox.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.nicknametextbox.DisabledState.PlaceholderForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(138)))), ((int)(((byte)(138)))), ((int)(((byte)(138)))));
            this.nicknametextbox.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.nicknametextbox.FocusedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.nicknametextbox.Font = new System.Drawing.Font("Segoe UI", 14.25F);
            this.nicknametextbox.ForeColor = System.Drawing.Color.White;
            this.nicknametextbox.HoverState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.nicknametextbox.Location = new System.Drawing.Point(640, 378);
            this.nicknametextbox.Margin = new System.Windows.Forms.Padding(4);
            this.nicknametextbox.Name = "nicknametextbox";
            this.nicknametextbox.PasswordChar = '\0';
            this.nicknametextbox.PlaceholderForeColor = System.Drawing.Color.Black;
            this.nicknametextbox.PlaceholderText = "";
            this.nicknametextbox.SelectedText = "";
            this.nicknametextbox.Size = new System.Drawing.Size(260, 48);
            this.nicknametextbox.TabIndex = 26;
            this.nicknametextbox.TextChanged += new System.EventHandler(this.nicknametextbox_TextChanged);
            // 
            // guna2ControlBox1
            // 
            this.guna2ControlBox1.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.guna2ControlBox1.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.guna2ControlBox1.IconColor = System.Drawing.Color.White;
            this.guna2ControlBox1.Location = new System.Drawing.Point(1491, 0);
            this.guna2ControlBox1.Name = "guna2ControlBox1";
            this.guna2ControlBox1.Size = new System.Drawing.Size(45, 29);
            this.guna2ControlBox1.TabIndex = 25;
            this.guna2ControlBox1.Click += new System.EventHandler(this.guna2ControlBox1_Click);
            // 
            // pictureBox1
            // 
            this.pictureBox1.BackColor = System.Drawing.Color.Transparent;
            this.pictureBox1.Image = global::Projects_Launcher.Properties.Resources.pprojectss;
            this.pictureBox1.Location = new System.Drawing.Point(655, 318);
            this.pictureBox1.Name = "pictureBox1";
            this.pictureBox1.Size = new System.Drawing.Size(230, 53);
            this.pictureBox1.TabIndex = 35;
            this.pictureBox1.TabStop = false;
            // 
            // ProjectsLauncherLogin
            // 
            this.AutoScaleDimensions = new System.Drawing.SizeF(6F, 13F);
            this.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
            this.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(25)))), ((int)(((byte)(30)))), ((int)(((byte)(45)))));
            this.ClientSize = new System.Drawing.Size(1536, 864);
            this.Controls.Add(this.pictureBox1);
            this.Controls.Add(this.label6);
            this.Controls.Add(this.raminfo);
            this.Controls.Add(this.benihatırla);
            this.Controls.Add(this.label4);
            this.Controls.Add(this.label5);
            this.Controls.Add(this.label2);
            this.Controls.Add(this.label3);
            this.Controls.Add(this.girisyapbutton);
            this.Controls.Add(this.nicknametextbox);
            this.Controls.Add(this.guna2ControlBox1);
            this.FormBorderStyle = System.Windows.Forms.FormBorderStyle.None;
            this.Icon = ((System.Drawing.Icon)(resources.GetObject("$this.Icon")));
            this.MaximizeBox = false;
            this.MaximumSize = new System.Drawing.Size(1536, 864);
            this.MinimumSize = new System.Drawing.Size(1536, 864);
            this.Name = "ProjectsLauncherLogin";
            this.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
            this.Text = "Projects Launcher";
            this.Load += new System.EventHandler(this.ProjectsLauncherLogin_Load);
            ((System.ComponentModel.ISupportInitialize)(this.pictureBox1)).EndInit();
            this.ResumeLayout(false);
            this.PerformLayout();

        }

        #endregion

        private System.Windows.Forms.PictureBox pictureBox1;
        private Guna.UI2.WinForms.Guna2BorderlessForm guna2BorderlessForm1;
        private System.Windows.Forms.Label label6;
        private System.Windows.Forms.Label raminfo;
        private Guna.UI2.WinForms.Guna2CheckBox benihatırla;
        private System.Windows.Forms.Label label4;
        private System.Windows.Forms.Label label5;
        private System.Windows.Forms.Label label2;
        private System.Windows.Forms.Label label3;
        private Guna.UI2.WinForms.Guna2Button girisyapbutton;
        private Guna.UI2.WinForms.Guna2TextBox nicknametextbox;
        private Guna.UI2.WinForms.Guna2ControlBox guna2ControlBox1;
    }
}

