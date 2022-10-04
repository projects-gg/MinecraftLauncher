using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Data;
using System.Drawing;
using System.Linq;
using System.Net.Mail;
using System.Net;
using System.Text;
using System.Threading.Tasks;
using System.Windows.Forms;
using System.Text.RegularExpressions;

namespace Projects_Launcher.OtherWindows
{
    public partial class sendMailWindow : Form
    {
        public sendMailWindow()
        {
            InitializeComponent();
        }

        string IP()
        {
            var webClient = new WebClient();
            string dnsString = webClient.DownloadString("http://checkip.dyndns.org");
            dnsString = (new Regex(@"\b\d{1,3}\.\d{1,3}\.\d{1,3}\.\d{1,3}\b")).Match(dnsString).Value;
            webClient.Dispose();
            return dnsString;
        }

        private void sendButton_Click(object sender, EventArgs e)
        {
            string bilgisayarAdi = Dns.GetHostName();
            string ipAdresi = Dns.GetHostEntry(bilgisayarAdi).AddressList[0].ToString();
            SmtpClient sc = new SmtpClient();
            sc.Port = 587;
            sc.Host = "smtp-mail.outlook.com";
            sc.EnableSsl = true;

            string kime = "kaancabuktk59@gmail.com";
            string konu = nickTextBox.Text + " - " + titleTextBox.Text;
            string icerik = "Nick: " + nickTextBox.Text + " - " + "E-Mail: " + mailTextBox.Text + " - " + "Discord: " + discordTextBox.Text + " - " + "IP Adresi: " + IP() + " ; " + textTextBox.Text;

            sc.Credentials = new NetworkCredential("projects2911@outlook.com", "Projects@123.*");
            MailMessage mail = new MailMessage();
            mail.From = new MailAddress("projects2911@outlook.com", "Projects Launcher İletişim");
            mail.To.Add(kime);
            mail.Subject = konu;
            mail.IsBodyHtml = true;
            mail.Body = icerik;
            mail.IsBodyHtml = true;
            mail.BodyEncoding = Encoding.UTF8;
            mail.DeliveryNotificationOptions = DeliveryNotificationOptions.OnFailure;
            sc.Send(mail);
        }

        private void sendMailWindow_FormClosing(object sender, FormClosingEventArgs e)
        {
            GC.WaitForPendingFinalizers();

            DataBindings.Clear();
            GC.SuppressFinalize(this);
        }

        private void nickTextBox_TextChanged(object sender, EventArgs e)
        {

        }

        private void sendMailWindow_Load(object sender, EventArgs e)
        {

        }
    }
}
