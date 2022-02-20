
namespace Projects_Launcher.MainWindows
{
    partial class ProjectsLauncherMain
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
            System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(ProjectsLauncherMain));
            this.MainWindow = new Guna.UI2.WinForms.Guna2BorderlessForm(this.components);
            this.timer1 = new System.Windows.Forms.Timer(this.components);
            this.guna2Panel1 = new Guna.UI2.WinForms.Guna2Panel();
            this.oynabutton = new Guna.UI2.WinForms.Guna2Button();
            this.kurallar = new System.Windows.Forms.Label();
            this.guna2Panel2 = new Guna.UI2.WinForms.Guna2Panel();
            this.nickname1 = new System.Windows.Forms.Label();
            this.website = new Guna.UI2.WinForms.Guna2Button();
            this.surumsec = new Guna.UI2.WinForms.Guna2ComboBox();
            this.guna2PictureBox1 = new Guna.UI2.WinForms.Guna2PictureBox();
            this.guna2Panel1.SuspendLayout();
            this.guna2Panel2.SuspendLayout();
            ((System.ComponentModel.ISupportInitialize)(this.guna2PictureBox1)).BeginInit();
            this.SuspendLayout();
            // 
            // MainWindow
            // 
            this.MainWindow.ContainerControl = this;
            this.MainWindow.DockIndicatorTransparencyValue = 0.6D;
            this.MainWindow.TransparentWhileDrag = true;
            // 
            // timer1
            // 
            this.timer1.Interval = 750;
            this.timer1.Tick += new System.EventHandler(this.timer1_Tick);
            // 
            // guna2Panel1
            // 
            this.guna2Panel1.Controls.Add(this.oynabutton);
            this.guna2Panel1.Controls.Add(this.kurallar);
            this.guna2Panel1.Controls.Add(this.guna2Panel2);
            this.guna2Panel1.Controls.Add(this.website);
            this.guna2Panel1.Controls.Add(this.surumsec);
            this.guna2Panel1.Controls.Add(this.guna2PictureBox1);
            this.guna2Panel1.Dock = System.Windows.Forms.DockStyle.Fill;
            this.guna2Panel1.Location = new System.Drawing.Point(0, 0);
            this.guna2Panel1.Name = "guna2Panel1";
            this.guna2Panel1.Size = new System.Drawing.Size(817, 598);
            this.guna2Panel1.TabIndex = 0;
            this.guna2Panel1.Paint += new System.Windows.Forms.PaintEventHandler(this.guna2Panel1_Paint);
            // 
            // oynabutton
            // 
            this.oynabutton.Animated = true;
            this.oynabutton.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(1)))), ((int)(((byte)(135)))), ((int)(((byte)(65)))));
            this.oynabutton.DisabledState.BorderColor = System.Drawing.Color.DarkGray;
            this.oynabutton.DisabledState.CustomBorderColor = System.Drawing.Color.DarkGray;
            this.oynabutton.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(169)))), ((int)(((byte)(169)))), ((int)(((byte)(169)))));
            this.oynabutton.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(141)))), ((int)(((byte)(141)))), ((int)(((byte)(141)))));
            this.oynabutton.FillColor = System.Drawing.Color.Transparent;
            this.oynabutton.Font = new System.Drawing.Font("Microsoft Sans Serif", 18F, System.Drawing.FontStyle.Bold, System.Drawing.GraphicsUnit.Point, ((byte)(0)));
            this.oynabutton.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(253)))), ((int)(((byte)(253)))), ((int)(((byte)(253)))));
            this.oynabutton.Location = new System.Drawing.Point(285, 528);
            this.oynabutton.Name = "oynabutton";
            this.oynabutton.Size = new System.Drawing.Size(182, 45);
            this.oynabutton.TabIndex = 133;
            this.oynabutton.Text = "Oyna";
            this.oynabutton.Click += new System.EventHandler(this.oynabutton_Click);
            // 
            // kurallar
            // 
            this.kurallar.AutoSize = true;
            this.kurallar.BackColor = System.Drawing.Color.Transparent;
            this.kurallar.Font = new System.Drawing.Font("Segoe UI", 12.75F, System.Drawing.FontStyle.Underline, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.kurallar.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.kurallar.Location = new System.Drawing.Point(717, 566);
            this.kurallar.Name = "kurallar";
            this.kurallar.Size = new System.Drawing.Size(68, 23);
            this.kurallar.TabIndex = 107;
            this.kurallar.Text = "Kurallar\r\n";
            this.kurallar.Click += new System.EventHandler(this.kurallar_Click);
            // 
            // guna2Panel2
            // 
            this.guna2Panel2.BackColor = System.Drawing.Color.Transparent;
            this.guna2Panel2.Controls.Add(this.nickname1);
            this.guna2Panel2.ImeMode = System.Windows.Forms.ImeMode.Off;
            this.guna2Panel2.Location = new System.Drawing.Point(-1, 434);
            this.guna2Panel2.Name = "guna2Panel2";
            this.guna2Panel2.Size = new System.Drawing.Size(819, 72);
            this.guna2Panel2.TabIndex = 134;
            // 
            // nickname1
            // 
            this.nickname1.AutoSize = true;
            this.nickname1.BackColor = System.Drawing.Color.Transparent;
            this.nickname1.Font = new System.Drawing.Font("Segoe UI", 12F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.nickname1.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.nickname1.Location = new System.Drawing.Point(4, 25);
            this.nickname1.Name = "nickname1";
            this.nickname1.RightToLeft = System.Windows.Forms.RightToLeft.No;
            this.nickname1.Size = new System.Drawing.Size(749, 21);
            this.nickname1.TabIndex = 98;
            this.nickname1.Text = "Projects, sizin için eğlenceli çevrim içi oyun sunucuları oluşturur ve bunların a" +
    "dil olarak oynanmasını sağlar.";
            // 
            // website
            // 
            this.website.Animated = true;
            this.website.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(1)))), ((int)(((byte)(135)))), ((int)(((byte)(65)))));
            this.website.DisabledState.BorderColor = System.Drawing.Color.DarkGray;
            this.website.DisabledState.CustomBorderColor = System.Drawing.Color.DarkGray;
            this.website.DisabledState.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(169)))), ((int)(((byte)(169)))), ((int)(((byte)(169)))));
            this.website.DisabledState.ForeColor = System.Drawing.Color.FromArgb(((int)(((byte)(141)))), ((int)(((byte)(141)))), ((int)(((byte)(141)))));
            this.website.FillColor = System.Drawing.Color.Transparent;
            this.website.Font = new System.Drawing.Font("Segoe UI Semibold", 12F, System.Drawing.FontStyle.Bold);
            this.website.ForeColor = System.Drawing.Color.White;
            this.website.Location = new System.Drawing.Point(687, 528);
            this.website.Name = "website";
            this.website.Size = new System.Drawing.Size(118, 35);
            this.website.TabIndex = 106;
            this.website.Text = "Web Site";
            this.website.Click += new System.EventHandler(this.website_Click);
            // 
            // surumsec
            // 
            this.surumsec.BackColor = System.Drawing.Color.Transparent;
            this.surumsec.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(35)))), ((int)(((byte)(35)))), ((int)(((byte)(35)))));
            this.surumsec.DrawMode = System.Windows.Forms.DrawMode.OwnerDrawFixed;
            this.surumsec.DropDownStyle = System.Windows.Forms.ComboBoxStyle.DropDownList;
            this.surumsec.FillColor = System.Drawing.Color.FromArgb(((int)(((byte)(35)))), ((int)(((byte)(35)))), ((int)(((byte)(35)))));
            this.surumsec.FocusedColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.surumsec.FocusedState.BorderColor = System.Drawing.Color.FromArgb(((int)(((byte)(94)))), ((int)(((byte)(148)))), ((int)(((byte)(255)))));
            this.surumsec.Font = new System.Drawing.Font("Arial", 12.75F, System.Drawing.FontStyle.Regular, System.Drawing.GraphicsUnit.Point, ((byte)(162)));
            this.surumsec.ForeColor = System.Drawing.Color.WhiteSmoke;
            this.surumsec.ItemHeight = 30;
            this.surumsec.Items.AddRange(new object[] {
            "fabric-loader-0.13.2-1.18.1",
            "1.18.1",
            "1.17.1",
            "1.16.5",
            "1.15.2",
            "1.14.4",
            "1.13.2",
            "1.12.2",
            "1.11.2",
            "1.10.2",
            "1.9.4",
            "1.8.9",
            "1.7.10"});
            this.surumsec.Location = new System.Drawing.Point(3, 537);
            this.surumsec.Name = "surumsec";
            this.surumsec.Size = new System.Drawing.Size(151, 36);
            this.surumsec.TabIndex = 132;
            this.surumsec.SelectedIndexChanged += new System.EventHandler(this.surumsec_SelectedIndexChanged);
            // 
            // guna2PictureBox1
            // 
            this.guna2PictureBox1.Image = ((System.Drawing.Image)(resources.GetObject("guna2PictureBox1.Image")));
            this.guna2PictureBox1.ImageRotate = 0F;
            this.guna2PictureBox1.Location = new System.Drawing.Point(-1, 64);
            this.guna2PictureBox1.Name = "guna2PictureBox1";
            this.guna2PictureBox1.Size = new System.Drawing.Size(819, 442);
            this.guna2PictureBox1.SizeMode = System.Windows.Forms.PictureBoxSizeMode.Zoom;
            this.guna2PictureBox1.TabIndex = 131;
            this.guna2PictureBox1.TabStop = false;
            // 
            // ProjectsLauncherMain
            // 
            this.AutoScaleDimensions = new System.Drawing.SizeF(6F, 13F);
            this.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
            this.BackColor = System.Drawing.Color.FromArgb(((int)(((byte)(35)))), ((int)(((byte)(35)))), ((int)(((byte)(35)))));
            this.ClientSize = new System.Drawing.Size(817, 598);
            this.Controls.Add(this.guna2Panel1);
            this.FormBorderStyle = System.Windows.Forms.FormBorderStyle.None;
            this.Icon = ((System.Drawing.Icon)(resources.GetObject("$this.Icon")));
            this.Name = "ProjectsLauncherMain";
            this.StartPosition = System.Windows.Forms.FormStartPosition.CenterScreen;
            this.Text = "Anamenu";
            this.Load += new System.EventHandler(this.ProjectsLauncherMain_Load);
            this.guna2Panel1.ResumeLayout(false);
            this.guna2Panel1.PerformLayout();
            this.guna2Panel2.ResumeLayout(false);
            this.guna2Panel2.PerformLayout();
            ((System.ComponentModel.ISupportInitialize)(this.guna2PictureBox1)).EndInit();
            this.ResumeLayout(false);

        }

        #endregion

        private Guna.UI2.WinForms.Guna2BorderlessForm MainWindow;
        private System.Windows.Forms.Timer timer1;
        private Guna.UI2.WinForms.Guna2Panel guna2Panel1;
        private Guna.UI2.WinForms.Guna2Panel guna2Panel2;
        public System.Windows.Forms.Label kurallar;
        private Guna.UI2.WinForms.Guna2Button website;
        public System.Windows.Forms.Label nickname1;
        private Guna.UI2.WinForms.Guna2Button oynabutton;
        public Guna.UI2.WinForms.Guna2ComboBox surumsec;
        private Guna.UI2.WinForms.Guna2PictureBox guna2PictureBox1;
    }
}