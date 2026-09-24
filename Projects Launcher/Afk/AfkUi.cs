using System;
using System.Linq;
using System.Runtime.InteropServices;
using System.Windows.Forms;

namespace Projects_Launcher.Afk
{
    /// <summary>AFK ekranlarında paylaşılan küçük arayüz yardımcıları.</summary>
    public static class AfkUi
    {
        [DllImport("uxtheme.dll", CharSet = CharSet.Unicode)]
        private static extern int SetWindowTheme(IntPtr hWnd, string pszSubAppName, string pszSubIdList);

        /// <summary>
        /// Guna2TextBox aslında içinde gerçek bir TextBox barındıran bir UserControl'dür ve klavye
        /// olaylarını dışarıya yeniden yaymaz: odak iç kutudayken kabuğun KeyDown'ı hiç tetiklenmez.
        /// Bu yüzden dinleyici, varsa doğrudan iç TextBox'a bağlanır.
        /// </summary>
        public static void AttachKeyDown(Control textBox, KeyEventHandler handler)
        {
            TextBox inner = textBox.Controls.OfType<TextBox>().FirstOrDefault();

            if (inner != null)
                inner.KeyDown += handler;
            else
                textBox.KeyDown += handler;
        }

        /// <summary>
        /// Kaydırılabilir kontrolün yerel kaydırma çubuğunu koyu temaya alır. Koyu ekranlarda beyaz
        /// kaydırma çubuğu sırıtıyordu. Windows 10 (1809) ve sonrasında geçerlidir; daha eski
        /// sürümler temayı tanımaz ve sessizce varsayılan çubuğu çizer. Tanıtıcı yeniden
        /// oluşturulursa tema kaybolmasın diye HandleCreated'da da uygulanır.
        /// </summary>
        public static void UseDarkScrollbars(Control control)
        {
            control.HandleCreated += delegate { ApplyDarkTheme(control); };

            if (control.IsHandleCreated)
                ApplyDarkTheme(control);
        }

        private static void ApplyDarkTheme(Control control)
        {
            try
            {
                SetWindowTheme(control.Handle, "DarkMode_Explorer", null);
            }
            catch (Exception)
            {
                // uxtheme bulunamazsa (çok eski sistem) varsayılan görünümle devam edilir.
            }
        }
    }
}
