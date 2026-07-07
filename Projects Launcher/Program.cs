using System;
using System.Text.RegularExpressions;
using System.Windows.Forms;

namespace Projects_Launcher
{
    static class Program
    {
        /// <summary>
        /// Uygulamanın ana girdi noktası.
        /// </summary>
        [STAThread]
        static void Main()
        {
            Application.EnableVisualStyles();
            Application.SetCompatibleTextRenderingDefault(false);

            // Kullanıcı adı daha önce kaydedildiyse giriş ekranı atlanır, ana menü doğrudan açılır.
            // Güncelleme denetimi normalde giriş ekranında yapıldığından ana menüye devredilir.
            string savedNick = Properties.Settings.Default.NickNames;
            if (!string.IsNullOrEmpty(savedNick) && Regex.IsMatch(savedNick, "^[a-zA-Z0-9_]+$"))
            {
                Application.Run(new Projects_Launcher.mainMenuForm(checkForUpdates: true));
            }
            else
            {
                Application.Run(new loginMenuForm());
            }
        }
    }
}
