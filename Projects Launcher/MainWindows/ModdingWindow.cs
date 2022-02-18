using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Data;
using System.Drawing;
using System.Linq;
using System.Diagnostics;
using System.IO;
using System.Text;
using System.Threading.Tasks;
using System.Windows.Forms;
using CmlLib;
using CmlLib.Core.Auth;
using CmlLib.Core;
using CmlLib.Core.Downloader;
using CmlLib.Core.Installer;
using CmlLib.Core.Files;
using System.Threading;
using System.Net;
using HtmlAgilityPack;


namespace Projects_Launcher
{
    public partial class ProjectsLauncherDownload : Form
    {
        public ProjectsLauncherDownload()
        {
            InitializeComponent();
        }
        Uri modpackurl = new Uri("https://www.dropbox.com/s/77vvegpkjxsold1/mods.zip?dl=1");
        Uri fabric = new Uri("https://www.dropbox.com/s/agaj6ootu3cmvok/fabric-installer-0.10.2.jar?dl=1");
        string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
        private void ProjectsLauncherDownload_Load(object sender, EventArgs e)
        {

        }

        private void indirbutton_Click(object sender, EventArgs e)
        {
            WebClient wc = new WebClient();
            wc.DownloadProgressChanged += Wc_DownloadProgressChanged;
            wc.DownloadFileCompleted += Wc_DownloadFileCompleted;
            wc.DownloadFileAsync(fabric, appDataDizini + "/.projects/fabric-installer-0.10.2.jar");




            // System.Threading.Thread.Sleep(2000);
        }

        private void indirbutton2_Click(object sender, EventArgs e)
        {
            WebClient wc = new WebClient();
            wc.DownloadProgressChanged += Wc_DownloadProgressChanged2;
            wc.DownloadFileCompleted += Wc_DownloadFileCompleted2;
            wc.DownloadFileAsync(modpackurl, appDataDizini + "/.projects/mods.zip");
        }

        private void Wc_DownloadFileCompleted(object sender, AsyncCompletedEventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/fabric-installer-0.10.2.jar";

            string myPath = @appDataDizini;
            System.Diagnostics.Process prc = new System.Diagnostics.Process();
            prc.StartInfo.FileName = myPath;
            System.Threading.Thread.Sleep(1000);
            prc.Start();


        }

        private void Wc_DownloadProgressChanged(object sender, DownloadProgressChangedEventArgs e)
        {
            yuzde1.Value = e.ProgressPercentage;
            yuzde1.Text = "%0" + e.BytesReceived;

        }

        private void Wc_DownloadFileCompleted2(object sender, AsyncCompletedEventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.projects/mods.zip";

            string myPath = @appDataDizini;
            System.Diagnostics.Process prc = new System.Diagnostics.Process();
            prc.StartInfo.FileName = myPath;
            System.Threading.Thread.Sleep(1000);
            prc.Start();


        }

        private void Wc_DownloadProgressChanged2(object sender, DownloadProgressChangedEventArgs e)
        {
            yuzde2.Value = e.ProgressPercentage;
            yuzde2.Text = "%0" + e.BytesReceived;




        }
    }
}
//