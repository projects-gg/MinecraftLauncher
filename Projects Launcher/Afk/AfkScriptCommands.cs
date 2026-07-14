using System;
using System.Collections.Generic;

namespace Projects_Launcher.Afk
{
    /// <summary>
    /// Konsolda betik (giriş) komutlarının gizlenmesiyle ilgili ortak kurallar.
    /// Parola taşıyan giriş/kayıt komutları "Betik komutlarını göster" açık olsa bile
    /// konsolda asla açığa çıkmaz; bu sınıf hangi komutların hassas sayılacağını belirler.
    /// </summary>
    public static class AfkScriptCommands
    {
        // İlk sözcüğü (komut adı) bu kümede olan her komut hassas sayılır ve konsoldan gizlemeden
        // çıkarılamaz. Karşılaştırma tam eşleşmedir: kısa takma adlar (/g, /l, /k) yalnızca kendileriyle
        // eşleşir, /give · /list · /kill gibi zararsız komutları etkilemez.
        private static readonly HashSet<string> SensitiveNames = new HashSet<string>(StringComparer.OrdinalIgnoreCase)
        {
            // Giriş
            "/login", "/l", "/log", "/signin",
            "/g", "/gir", "/giris", "/giriş", "/girisyap", "/girişyap",
            // Kayıt
            "/register", "/reg", "/signup",
            "/k", "/kayit", "/kayıt", "/kaydol",
            // Parola / kimlik
            "/password", "/pass", "/passwd", "/sifre", "/şifre",
            "/changepassword", "/changepass", "/cp",
            "/auth", "/authme", "/2fa", "/2fauth",
        };

        /// <summary>Komutun ilk sözcüğü hassas giriş/kayıt/parola komutlarından biriyse true.</summary>
        public static bool IsSensitive(string command)
        {
            string name = FirstToken(command);
            return name.Length > 0 && SensitiveNames.Contains(name);
        }

        private static string FirstToken(string command)
        {
            if (string.IsNullOrEmpty(command))
                return string.Empty;

            string trimmed = command.Trim();
            int split = trimmed.IndexOfAny(new[] { ' ', '\t' });
            return split < 0 ? trimmed : trimmed.Substring(0, split);
        }
    }
}
