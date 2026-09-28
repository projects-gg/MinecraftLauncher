using System;
using System.Collections.Generic;
using System.Drawing;
using System.Linq;
using System.Runtime.InteropServices;
using System.Windows.Forms;
using Guna.UI2.WinForms;

namespace Projects_Launcher.Afk
{
    /// <summary>AFK ekranlarında paylaşılan küçük arayüz yardımcıları.</summary>
    public static class AfkUi
    {
        [DllImport("uxtheme.dll", CharSet = CharSet.Unicode)]
        private static extern int SetWindowTheme(IntPtr hWnd, string pszSubAppName, string pszSubIdList);

        [DllImport("user32.dll")]
        private static extern bool GetComboBoxInfo(IntPtr hwndCombo, ref ComboBoxInfo info);

        [DllImport("user32.dll")]
        private static extern bool InvalidateRect(IntPtr hWnd, IntPtr lpRect, bool bErase);

        [StructLayout(LayoutKind.Sequential)]
        private struct NativeRect
        {
            public int Left, Top, Right, Bottom;
        }

        [StructLayout(LayoutKind.Sequential)]
        private struct ComboBoxInfo
        {
            public int cbSize;
            public NativeRect rcItem;
            public NativeRect rcButton;
            public int stateButton;
            public IntPtr hwndCombo;
            public IntPtr hwndItem;
            public IntPtr hwndList;
        }

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

        /// <summary>
        /// Koyu ekranlardaki açılır listeler için ortak görünüm: satırlar kutunun kendi renkleriyle,
        /// üzerine gelinen/seçili satır belirgin bir tonla çizilir; liste en fazla
        /// <paramref name="visibleItems"/> satır gösterir ve kaydırma düzeltmesi uygulanır.
        /// </summary>
        public static void StyleDropDown(Guna2ComboBox combo, int visibleItems)
        {
            combo.ItemsAppearance.BackColor = combo.FillColor;
            combo.ItemsAppearance.ForeColor = combo.ForeColor;
            combo.ItemsAppearance.SelectedBackColor = Color.FromArgb(64, 74, 94);
            combo.ItemsAppearance.SelectedForeColor = Color.White;
            combo.IntegralHeight = true;
            combo.MaxDropDownItems = visibleItems;
            FixDropDownList(combo);
        }

        /// <summary>
        /// Guna2ComboBox satırları kendisi çizer; açılır liste kaydırıldığında ise kaydırma çubuğu
        /// ilerlediği hâlde görünen satırlar yeniden çizilmiyor, eski satırlar ekranda kalıp birbirine
        /// karışıyordu. Liste penceresi dinlenir ve kaydırmaya yol açan her iletiden sonra tamamı
        /// yeniden çizdirilir. Listenin kaydırma çubuğu da koyu temaya alınır.
        /// </summary>
        public static void FixDropDownList(ComboBox combo)
        {
            combo.DropDown += delegate
            {
                try
                {
                    ComboBoxInfo info = new ComboBoxInfo();
                    info.cbSize = Marshal.SizeOf(typeof(ComboBoxInfo));

                    if (GetComboBoxInfo(combo.Handle, ref info) && info.hwndList != IntPtr.Zero)
                    {
                        SetWindowTheme(info.hwndList, "DarkMode_Explorer", null);
                        DropDownListRepainter.Attach(info.hwndList);
                    }
                }
                catch (Exception)
                {
                    // Liste penceresine erişilemezse varsayılan davranışla devam edilir.
                }
            };
        }

        private sealed class DropDownListRepainter : NativeWindow
        {
            private const int WM_NCDESTROY = 0x0082;
            private const int WM_TIMER = 0x0113;
            private const int WM_VSCROLL = 0x0115;
            private const int WM_MOUSEWHEEL = 0x020A;
            private const int LB_SETCURSEL = 0x0186;
            private const int LB_SETTOPINDEX = 0x0197;

            // NativeWindow örneği toplanmasın diye liste penceresi yaşadıkça burada tutulur.
            private static readonly Dictionary<IntPtr, DropDownListRepainter> Attached =
                new Dictionary<IntPtr, DropDownListRepainter>();

            private readonly IntPtr listHandle;

            private DropDownListRepainter(IntPtr listHandle)
            {
                this.listHandle = listHandle;
                AssignHandle(listHandle);
            }

            public static void Attach(IntPtr listHandle)
            {
                if (!Attached.ContainsKey(listHandle))
                    Attached[listHandle] = new DropDownListRepainter(listHandle);
            }

            protected override void WndProc(ref Message m)
            {
                if (m.Msg == WM_NCDESTROY)
                    Attached.Remove(listHandle);

                base.WndProc(ref m);

                switch (m.Msg)
                {
                    case WM_VSCROLL:
                    case WM_MOUSEWHEEL:
                    case WM_TIMER: // basılı tutup kenara sürüklerken otomatik kaydırma
                    case LB_SETCURSEL:
                    case LB_SETTOPINDEX:
                        InvalidateRect(listHandle, IntPtr.Zero, true);
                        break;
                }
            }
        }
    }
}
