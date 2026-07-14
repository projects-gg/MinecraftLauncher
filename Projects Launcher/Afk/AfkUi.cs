using System.Linq;
using System.Windows.Forms;

namespace Projects_Launcher.Afk
{
    /// <summary>AFK ekranlarında paylaşılan küçük arayüz yardımcıları.</summary>
    public static class AfkUi
    {
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
    }
}
