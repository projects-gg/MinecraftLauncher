using System.Drawing;

namespace Projects_Launcher.Auth
{
    /// <summary>
    /// Microsoft'un dört kareli logosu. Kaynak dosyası eklemek yerine çizilir: her boyutta net
    /// görünür ve yayın paketine ayrı bir görsel girmez.
    /// </summary>
    public static class MicrosoftBrand
    {
        private static readonly Color Red = Color.FromArgb(242, 80, 34);
        private static readonly Color Green = Color.FromArgb(127, 186, 0);
        private static readonly Color Blue = Color.FromArgb(0, 164, 239);
        private static readonly Color Yellow = Color.FromArgb(255, 185, 0);

        /// <summary>Verilen kenar uzunluğunda saydam zeminli logo üretir. Çağıran tarafın işi bitince <c>Dispose</c> etmesi gerekir.</summary>
        public static Bitmap CreateLogo(int size)
        {
            Bitmap bitmap = new Bitmap(size, size);

            using (Graphics graphics = Graphics.FromImage(bitmap))
            {
                graphics.Clear(Color.Transparent);

                int gap = size >= 16 ? 2 : 1;
                int cell = (size - gap) / 2;

                using (SolidBrush red = new SolidBrush(Red))
                using (SolidBrush green = new SolidBrush(Green))
                using (SolidBrush blue = new SolidBrush(Blue))
                using (SolidBrush yellow = new SolidBrush(Yellow))
                {
                    graphics.FillRectangle(red, 0, 0, cell, cell);
                    graphics.FillRectangle(green, cell + gap, 0, cell, cell);
                    graphics.FillRectangle(blue, 0, cell + gap, cell, cell);
                    graphics.FillRectangle(yellow, cell + gap, cell + gap, cell, cell);
                }
            }

            return bitmap;
        }
    }
}
