using System;
using System.IO;
using Newtonsoft.Json;

namespace Projects_Launcher.Afk
{
    /// <summary>
    /// AFK ekranının hesaptan bağımsız genel ayarları (settings.json). Şimdilik tek anahtar var:
    /// betik komutlarının konsolda gösterilip gösterilmeyeceği (varsayılan: gizli). Değer değişince
    /// o an açık olan tüm konsolların kendini tazeleyebilmesi için <see cref="Changed"/> tetiklenir.
    /// </summary>
    public static class AfkSettings
    {
        private class Data
        {
            // Varsayılan false: betik komutları konsolda gizli başlar.
            public bool ShowScriptCommands { get; set; }
        }

        private static readonly object gate = new object();
        private static Data cache;

        /// <summary>Herhangi bir ayar değiştiğinde, değeri değiştiren iş parçacığında tetiklenir.</summary>
        public static event EventHandler Changed;

        /// <summary>
        /// Betik (giriş) komutları konsolda görünsün mü? Kapalıyken tüm betik komutları gizlenir;
        /// açıkken hassas olmayanlar görünür, giriş/kayıt/parola komutları yine gizli kalır.
        /// </summary>
        public static bool ShowScriptCommands
        {
            get { return Current().ShowScriptCommands; }
            set
            {
                lock (gate)
                {
                    Data data = Current();
                    if (data.ShowScriptCommands == value)
                        return;

                    data.ShowScriptCommands = value;
                    SaveLocked(data);
                }

                EventHandler handler = Changed;
                if (handler != null)
                    handler(null, EventArgs.Empty);
            }
        }

        private static Data Current()
        {
            lock (gate)
            {
                if (cache == null)
                    cache = Load();
                return cache;
            }
        }

        private static Data Load()
        {
            try
            {
                if (File.Exists(AfkPaths.SettingsFile))
                {
                    Data data = JsonConvert.DeserializeObject<Data>(File.ReadAllText(AfkPaths.SettingsFile));
                    if (data != null)
                        return data;
                }
            }
            catch (Exception)
            {
                // Bozuk ya da okunamayan ayar dosyası sessizce varsayılanlara düşer.
            }

            return new Data();
        }

        private static void SaveLocked(Data data)
        {
            try
            {
                Directory.CreateDirectory(AfkPaths.Root);
                File.WriteAllText(AfkPaths.SettingsFile, JsonConvert.SerializeObject(data, Formatting.Indented));
            }
            catch (Exception)
            {
                // Diske yazılamazsa ayar bu oturum boyunca bellek içinde geçerli kalır.
            }
        }
    }
}
