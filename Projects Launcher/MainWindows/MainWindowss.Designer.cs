
namespace Projects_Launcher.MainWindows
{
    partial class ProjectsMain
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

        /// <summary>
        /// Required method for Designer support - do not modify
        /// the contents of this method with the code editor.
        /// </summary>
        private void InitializeComponent()
        {
            this.components = new System.ComponentModel.Container();
            System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(ProjectsMain));
            this.nickname1 = new System.Windows.Forms.Label();
            this.timer2 = new System.Windows.Forms.Timer(this.components);
            this.timer1 = new System.Windows.Forms.Timer(this.components);
            this.timer3 = new System.Windows.Forms.Timer(this.components);
            this.guna2ControlBox1 = new Guna.UI2.WinForms.Guna2ControlBox();
            this.guna2Panel1 = new Guna.UI2.WinForms.Guna2Panel();
            this.guna2PictureBox3 = new Guna.UI2.WinForms.Guna2PictureBox();
            this.guna2Panel2 = new Guna.UI2.WinForms.Guna2Panel();
            this.surumchangelogs = new Guna.UI2.WinForms.Guna2Button();
            this.ayarlarbutton = new Guna.UI2.WinForms.Guna2Button();
            this.anamenü = new Guna.UI2.WinForms.Guna2Button();
            this.Formpanel = new Guna.UI2.WinForms.Guna2Panel();
            this.pictureBox1 = new System.Windows.Forms.PictureBox();
            this.ramlabel = new System.Windows.Forms.Label();
            this.heightlabel = new System.Windows.Forms.Label();
            this.widthlabel = new System.Windows.Forms.Label();
            this.surumtext = new System.Windows.Forms.Label();
            this.serverstatuss = new System.Windows.Forms.Label();
            this.guna2Panel1.SuspendLayout();
            ((System.ComponentModel.ISupportInitialize)(this.guna2PictureBox3)).BeginInit();
            this.guna2Panel2.SuspendLayout();
            ((System.ComponentModel.ISupportInitialize)(this.pictureBox1)).BeginInit();
            this.SuspendLayout();
            // 
            // nickname1
            // 
            this.nickname1.AutoSize = true;
            this.nickname1.BackColor = System.Drawing.Color.Transparent;
            this.nickname1.Font = new System.Drawing.Font("Segoe UI", 11.25F);
            this.nickname1.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.nickname1.Location = new System.Drawing.Point(33, 3);
            this.nickname1.Name = "nickname1";
            this.nickname1.Size = new System.Drawing.Size(82, 20);
            this.nickname1.TabIndex = 97;
            this.nickname1.Text = "{nickname}";
            // 
            // timer2
            // 
            this.timer2.Interval = 750;
            this.timer2.Tick += new System.EventHandler(this.timer2_Tick_1);
            // 
            // timer1
            // 
            this.timer1.Enabled = true;
            // 
            // timer3
            // 
            this.timer3.Interval = 750;
            this.timer3.Tick += new System.EventHandler(this.timer3_Tick);
            // 
            // guna2ControlBox1
            // 
            this.guna2ControlBox1.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.guna2ControlBox1.Animated = true;
            this.guna2ControlBox1.BackColor = System.Drawing.Color.Transparent;
            this.guna2ControlBox1.FillColor = System.Drawing.Color.Transparent;
            this.guna2ControlBox1.ForeColor = System.Drawing.Color.Transparent;
            this.guna2ControlBox1.IconColor = System.Drawing.Color.White;
            this.guna2ControlBox1.Location = new System.Drawing.Point(953, 0);
            this.guna2ControlBox1.Name = "guna2ControlBox1";
            this.guna2ControlBox1.Size = new System.Drawing.Size(45, 30);
            this.guna2ControlBox1.TabIndex = 110;
            this.guna2ControlBox1.Click += new System.EventHandler(this.guna2ControlBox1_Click);
            // 
            // guna2Panel1
            // 
            this.guna2Panel1.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.guna2Panel1.Controls.Add(this.guna2PictureBox3);
            this.guna2Panel1.Controls.Add(this.nickname1);
            this.guna2Panel1.Location = new System.Drawing.Point(0, 31);
            this.guna2Panel1.Name = "guna2Panel1";
            this.guna2Panel1.Size = new System.Drawing.Size(182, 44);
            this.guna2Panel1.TabIndex = 114;
            // 
            // guna2PictureBox3
            // 
            this.guna2PictureBox3.ImageRotate = 0F;
            this.guna2PictureBox3.Location = new System.Drawing.Point(3, 3);
            this.guna2PictureBox3.Name = "guna2PictureBox3";
            this.guna2PictureBox3.Size = new System.Drawing.Size(24, 24);
            this.guna2PictureBox3.TabIndex = 113;
            this.guna2PictureBox3.TabStop = false;
            // 
            // guna2Panel2
            // 
            this.guna2Panel2.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.guna2Panel2.Controls.Add(this.surumchangelogs);
            this.guna2Panel2.Controls.Add(this.ayarlarbutton);
            this.guna2Panel2.Controls.Add(this.anamenü);
            this.guna2Panel2.Location = new System.Drawing.Point(0, 77);
            this.guna2Panel2.Name = "guna2Panel2";
            this.guna2Panel2.Size = new System.Drawing.Size(182, 555);
            this.guna2Panel2.TabIndex = 115;
            // 
            // surumchangelogs
            // 
            this.surumchangelogs.Animated = true;
            this.surumchangelogs.DisabledState.BorderColor = System.Drawing.Color.DarkGray;
            this.surumchangelogs.DisabledState.CustomBorderColor = System.Drawing.Color.DarkGray;
            this.surumchangelogs.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(169)))), ((int)(((byte)(169)))), ((int)(((byte)(169)))));
            this.surumchangelogs.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(141)))), ((int)(((byte)(141)))), ((int)(((byte)(141)))));
            this.surumchangelogs.FillColor = System.Drawing.Color.Transparent;
            this.surumchangelogs.Font = new System.Drawing.Font("Segoe UI Semibold", 12F, System.Drawing.FontStyle.Bold);
            this.surumchangelogs.ForeColor = System.Drawing.Color.White;
            this.surumchangelogs.Location = new System.Drawing.Point(0, 502);
            this.surumchangelogs.Name = "surumchangelogs";
            this.surumchangelogs.Size = new System.Drawing.Size(182, 53);
            this.surumchangelogs.TabIndex = 105;
            this.surumchangelogs.Text = "Changelogs";
            this.surumchangelogs.Click += new System.EventHandler(this.surumchangelogs_Click);
            // 
            // ayarlarbutton
            // 
            this.ayarlarbutton.Animated = true;
            this.ayarlarbutton.DisabledState.BorderColor = System.Drawing.Color.DarkGray;
            this.ayarlarbutton.DisabledState.CustomBorderColor = System.Drawing.Color.DarkGray;
            this.ayarlarbutton.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(169)))), ((int)(((byte)(169)))), ((int)(((byte)(169)))));
            this.ayarlarbutton.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(141)))), ((int)(((byte)(141)))), ((int)(((byte)(141)))));
            this.ayarlarbutton.FillColor = System.Drawing.Color.Transparent;
            this.ayarlarbutton.Font = new System.Drawing.Font("Arial", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.ayarlarbutton.ForeColor = System.Drawing.Color.White;
            this.ayarlarbutton.Location = new System.Drawing.Point(0, 72);
            this.ayarlarbutton.Name = "ayarlarbutton";
            this.ayarlarbutton.Size = new System.Drawing.Size(182, 53);
            this.ayarlarbutton.TabIndex = 1;
            this.ayarlarbutton.Text = "Ayarlar";
            this.ayarlarbutton.Click += new System.EventHandler(this.ayarlarbutton_Click);
            // 
            // anamenü
            // 
            this.anamenü.Animated = true;
            this.anamenü.DisabledState.BorderColor = System.Drawing.Color.DarkGray;
            this.anamenü.DisabledState.CustomBorderColor = System.Drawing.Color.DarkGray;
            this.anamenü.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(169)))), ((int)(((byte)(169)))), ((int)(((byte)(169)))));
            this.anamenü.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(141)))), ((int)(((byte)(141)))), ((int)(((byte)(141)))));
            this.anamenü.FillColor = System.Drawing.Color.Transparent;
            this.anamenü.Font = new System.Drawing.Font("Segoe UI Semibold", 12F, System.Drawing.FontStyle.Bold);
            this.anamenü.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(250)))), ((int)(((byte)(235)))), ((int)(((byte)(245)))));
            this.anamenü.Location = new System.Drawing.Point(0, 13);
            this.anamenü.Name = "anamenü";
            this.anamenü.Size = new System.Drawing.Size(182, 53);
            this.anamenü.TabIndex = 0;
            this.anamenü.Text = "Ana Menü";
            this.anamenü.Click += new System.EventHandler(this.anamenü_Click);
            // 
            // Formpanel
            // 
            this.Formpanel.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(35)))), ((int)(((byte)(35)))), ((int)(((byte)(35)))));
            this.Formpanel.Location = new System.Drawing.Point(181, 31);
            this.Formpanel.Name = "Formpanel";
            this.Formpanel.Size = new System.Drawing.Size(817, 598);
            this.Formpanel.TabIndex = 116;
            // 
            // pictureBox1
            // 
            this.pictureBox1.Image = ((System.Drawing.Image)(resources.GetObject("pictureBox1.Image")));
            this.pictureBox1.Location = new System.Drawing.Point(752, 0);
            this.pictureBox1.Name = "pictureBox1";
            this.pictureBox1.Size = new System.Drawing.Size(195, 30);
            this.pictureBox1.TabIndex = 105;
            this.pictureBox1.TabStop = false;
            // 
            // ramlabel
            // 
            this.ramlabel.AutoSize = true;
            this.ramlabel.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.ramlabel.ForeColor = System.Drawing.SystemColors.Control;
            this.ramlabel.Location = new System.Drawing.Point(3, 0);
            this.ramlabel.Name = "ramlabel";
            this.ramlabel.Size = new System.Drawing.Size(46, 13);
            this.ramlabel.TabIndex = 117;
            this.ramlabel.Text = "ramlabel";
            this.ramlabel.Visible = false;
            // 
            // heightlabel
            // 
            this.heightlabel.AutoSize = true;
            this.heightlabel.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.heightlabel.ForeColor = System.Drawing.SystemColors.Control;
            this.heightlabel.Location = new System.Drawing.Point(115, 0);
            this.heightlabel.Name = "heightlabel";
            this.heightlabel.Size = new System.Drawing.Size(58, 13);
            this.heightlabel.TabIndex = 118;
            this.heightlabel.Text = "heightlabel";
            this.heightlabel.Visible = false;
            // 
            // widthlabel
            // 
            this.widthlabel.AutoSize = true;
            this.widthlabel.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.widthlabel.ForeColor = System.Drawing.SystemColors.Control;
            this.widthlabel.Location = new System.Drawing.Point(55, 0);
            this.widthlabel.Name = "widthlabel";
            this.widthlabel.Size = new System.Drawing.Size(54, 13);
            this.widthlabel.TabIndex = 119;
            this.widthlabel.Text = "widthlabel";
            this.widthlabel.Visible = false;
            // 
            // surumtext
            // 
            this.surumtext.AutoSize = true;
            this.surumtext.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(36)))), ((int)(((byte)(36)))), ((int)(((byte)(36)))));
            this.surumtext.ForeColor = System.Drawing.SystemColors.Control;
            this.surumtext.Location = new System.Drawing.Point(178, 0);
            this.surumtext.Name = "surumtext";
            this.surumtext.Size = new System.Drawing.Size(52, 13);
            this.surumtext.TabIndex = 120;
            this.surumtext.Text = "surumtext";
            this.surumtext.Visible = false;
            this.surumtext.Click += new System.EventHandler(this.surumtext_Click);
            // 
            // serverstatuss
            // 
            this.serverstatuss.AutoSize = true;
            this.serverstatuss.ForeColor = System.Drawing.SystemColors.Control;
            this.serverstatuss.Location = new System.Drawing.Point(593, 0);
            this.serverstatuss.Name = "serverstatuss";
            this.serverstatuss.Size = new System.Drawing.Size(72, 13);
            this.serverstatuss.TabIndex = 121;
            this.serverstatuss.Text = "{serverstatus}";
            this.serverstatuss.Visible = false;
            // 
            // ProjectsMain
            // 
            this.AutoScaleDimensions = new System.Drawing.SizeF(6F, 13F);
            this.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
            this.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(35)))), ((int)(((byte)(35)))), ((int)(((byte)(35)))));
            this.ClientSize = new System.Drawing.Size(1000, 630);
            this.Controls.Add(this.serverstatuss);
            this.Controls.Add(this.surumtext);
            this.Controls.Add(this.widthlabel);
            this.Controls.Add(this.heightlabel);
            this.Controls.Add(this.ramlabel);
            this.Controls.Add(this.Formpanel);
            this.Controls.Add(this.guna2Panel2);
            this.Controls.Add(this.guna2Panel1);
            this.Controls.Add(this.guna2ControlBox1);
            this.Controls.Add(this.pictureBox1);
            this.FormBorderStyle = System.Windows.Forms.FormBorderStyle.None;
            this.Icon = ((System.Drawing.Icon)(resources.GetObject("$this.Icon")));
            this.Name = "ProjectsMain";
            this.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
            this.Text = "Projects Launcher";
            this.Load += new System.EventHandler(this.MainWindowss_Load);
            this.guna2Panel1.ResumeLayout(false);
            this.guna2Panel1.PerformLayout();
            ((System.ComponentModel.ISupportInitialize)(this.guna2PictureBox3)).EndInit();
            this.guna2Panel2.ResumeLayout(false);
            ((System.ComponentModel.ISupportInitialize)(this.pictureBox1)).EndInit();
            this.ResumeLayout(false);
            this.PerformLayout();

        }

        #endregion
        private System.Windows.Forms.PictureBox pictureBox1;
        private System.Windows.Forms.Timer timer2;
        private System.Windows.Forms.Timer timer1;
        private System.Windows.Forms.Timer timer3;
        private Guna.UI2.WinForms.Guna2ControlBox guna2ControlBox1;
        private Guna.UI2.WinForms.Guna2PictureBox guna2PictureBox3;
        private Guna.UI2.WinForms.Guna2Panel guna2Panel1;
        private Guna.UI2.WinForms.Guna2Panel guna2Panel2;
        private Guna.UI2.WinForms.Guna2Button anamenü;
        private Guna.UI2.WinForms.Guna2Button ayarlarbutton;
        private Guna.UI2.WinForms.Guna2Button surumchangelogs;
        private Guna.UI2.WinForms.Guna2Panel Formpanel;
        private System.Windows.Forms.Label serverstatuss;
        public System.Windows.Forms.Label ramlabel;
        public System.Windows.Forms.Label heightlabel;
        public System.Windows.Forms.Label widthlabel;
        public System.Windows.Forms.Label surumtext;
        public System.Windows.Forms.Label nickname1;
    }
}