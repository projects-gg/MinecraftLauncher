﻿using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Data;
using System.Drawing;
using System.Linq;
using System.Text;
using System.Threading.Tasks;
using System.Windows.Forms;
using CmlLib;
using CmlLib.Core;
using CmlLib.Core.Auth;
using System.Threading;
using System.Deployment.Application;
using System.IO;
using HtmlAgilityPack;


namespace Projects_Launcher
{
    public partial class ProjectsLauncherLogin : Form
    {
        public ProjectsLauncherLogin()
        {
            InitializeComponent();
        }
        public static string nickname;
        private void ProjectsLauncherLogin_Load(object sender, EventArgs e)
        {
            if (Properties.Settings.Default.NickName != string.Empty)
            {
                nicknametextbox.Text = Properties.Settings.Default.NickName;
            }

            // try
            // {
            //     //ApplicationDeployment, güncelleştirme bilgilerine erişmemizi sağlayacak olan bir sınıftır.
            //     ApplicationDeployment ad = ApplicationDeployment.CurrentDeployment;
            //     //CheckForDetailedUpdate metodu ile güncelleme var mı? yok mu? kontrol ediyoruz.
            //    UpdateCheckInfo info = ad.CheckForDetailedUpdate();
            //   if (info.UpdateAvailable)
            //    {
            //       if (DialogResult.Yes == MessageBox.Show($@"Şu anki versiyonunuz: {ad.CurrentVersion.ToString()} Yeni versiyon: {info.AvailableVersion.ToString()} kullanılabilir durumda. Yüklemek istiyor musunuz?",
            //           "Bilgi",
            //           MessageBoxButtons.YesNo,
            //          MessageBoxIcon.Information,
            //            MessageBoxDefaultButton.Button1))
            //       {
            // //            if (ad.Update())
            //           {
            //             MessageBox.Show("Program Başarıyla Güncellendi. Şimdi yeniden Başlatılacak.");
            //            Application.Restart();
            //         }
            //         else
            //              MessageBox.Show("Güncelleme Sırasında Hata Oluştu");
            //       }
            //   }
            //   else
            //        MessageBox.Show("Güncelleme bulunmamaktadır.");
            // }
            // catch
            //{
            //    MessageBox.Show("Sunucuyla bağlantı sağlanamadı.");
            //}

        }

        private void benihatırla_CheckedChanged(object sender, EventArgs e)
        {
            if (benihatırla.Checked == true)
            {
                Properties.Settings.Default.NickName = nicknametextbox.Text;
                Properties.Settings.Default.Save();
            }
        }

        private void label3_Click(object sender, EventArgs e)
        {
            System.Diagnostics.Process.Start("https://mc.projects.gg/");
        }

        private void guna2ControlBox1_Click(object sender, EventArgs e)
        {
            Application.Exit();
        }

        private void girisyapbutton_Click(object sender, EventArgs e)
        {
            if (string.IsNullOrEmpty(nicknametextbox.Text))
            {
                MessageBox.Show("Her hangi bir Nick girmediniz.");
            }
            else
            {
                nickname = nicknametextbox.Text;
            }
            ProjectsLauncherMain main = new ProjectsLauncherMain();
            this.Hide();
            main.Show();
        }
    }
}