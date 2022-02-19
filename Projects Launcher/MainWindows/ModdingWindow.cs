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
using System.IO.Compression;


namespace Projects_Launcher
{
    public partial class ProjectsLauncherDownload : Form
    {
        public ProjectsLauncherDownload()
        {
            InitializeComponent();
        }
        Uri fabric = new Uri("https://www.dropbox.com/s/agaj6ootu3cmvok/fabric-installer-0.10.2.jar?dl=1");

        Uri modlar = new Uri("https://www.dropbox.com/sh/k7bwyfdywhgpr0m/AACZaJlPzx7sQ3QVTtPNecJMa?dl=1");
        string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);

        private void ProjectsLauncherDownload_Load(object sender, EventArgs e)
        {

        }

        private void indirbutton_Click(object sender, EventArgs e)
        {
            WebClient wc = new WebClient();
            wc.DownloadProgressChanged += Wc_DownloadProgressChanged3;
            wc.DownloadFileCompleted += Wc_DownloadFileCompleted3;
            wc.DownloadFileAsync(fabric, appDataDizini + "/.minecraft/fabric-installer-0.10.2.jar");
        }

        private void indirbutton2_Click(object sender, EventArgs e)
        {
            WebClient wc = new WebClient();
            wc.DownloadProgressChanged += Wc_DownloadProgressChanged4;
            wc.DownloadFileCompleted += Wc_DownloadFileCompleted4;
            wc.DownloadFileAsync(modlar, Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/mods.zip");
        }

        private void Wc_DownloadFileCompleted3(object sender, AsyncCompletedEventArgs e)
        {
            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/fabric-installer-0.10.2.jar";

            string myPath = @appDataDizini;
            System.Diagnostics.Process prc = new System.Diagnostics.Process();
            prc.StartInfo.FileName = myPath;
            System.Threading.Thread.Sleep(1000);
            prc.Start();


        }
        private void Wc_DownloadFileCompleted4(object sender, AsyncCompletedEventArgs e)
        {
         

            string appDataDizini = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData) + "/.minecraft/mods.zip";


            string myPath = @appDataDizini;
            System.Diagnostics.Process prc = new System.Diagnostics.Process();
            prc.StartInfo.FileName = myPath;
            System.Threading.Thread.Sleep(1000);
            prc.Start();


        }

        private void Wc_DownloadProgressChanged3(object sender, DownloadProgressChangedEventArgs e)
        {
            yuzde1.Value = e.ProgressPercentage;
            yuzde1.Text = "%0" + e.BytesReceived;

        }

       

        private void Wc_DownloadProgressChanged4(object sender, DownloadProgressChangedEventArgs e)
        {
            yuzde2.Value = e.ProgressPercentage;
            yuzde2.Text = "%0" + e.BytesReceived;




        }

        private void indirbutton2_Click_1(object sender, EventArgs e)
        {

        }
    }
}
//