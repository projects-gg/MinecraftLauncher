using System;
using System.Collections.Generic;
using System.IO;
using Newtonsoft.Json;

namespace Projects_Launcher.Afk
{
    /// <summary>
    /// AFK istemcisiyle ilgili tüm dosya yolları. Her hesap kendi klasöründe çalışır:
    /// MCC oturum önbelleğini, sohbet günlüğünü ve matches.ini'yi çalışma dizinine göreli
    /// yazdığı için hesapların birbirinin dosyasını ezmesi ancak böyle engellenir.
    /// </summary>
    public static class AfkPaths
    {
        private static string AppData
        {
            get { return Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData); }
        }

        public static string Root
        {
            get { return Path.Combine(AppData, ".projects", "afk"); }
        }

        public static string BinDir
        {
            get { return Path.Combine(Root, "bin"); }
        }

        // Dağıtılan AFK istemcisinin dosya adı. MCC tek dosya olarak yayımlanıp bu ada
        // yeniden adlandırılır (apphost dosya adından bağımsız çalışır); Görev Yöneticisi'nde
        // "MinecraftClient" yerine bu marka görünür.
        public const string ClientExeName = "Projects-Konsol-Hesap.exe";

        public static string ClientExe
        {
            get { return Path.Combine(BinDir, ClientExeName); }
        }

        /// <summary>Kurulu AFK istemcisinin sürümü. Yoksa istemci indirilmemiş demektir.</summary>
        public static string VersionFile
        {
            get { return Path.Combine(BinDir, "version.txt"); }
        }

        public static string AccountsFile
        {
            get { return Path.Combine(Root, "accounts.json"); }
        }

        /// <summary>Hesaptan bağımsız genel AFK ayarları (bkz. <see cref="AfkSettings"/>).</summary>
        public static string SettingsFile
        {
            get { return Path.Combine(Root, "settings.json"); }
        }

        public static string AccountDir(AfkAccount account)
        {
            return Path.Combine(Root, "accounts", account.Id);
        }

        public static string AccountIni(AfkAccount account)
        {
            return Path.Combine(AccountDir(account), "MinecraftClient.ini");
        }

        public static string AccountMatchesFile(AfkAccount account)
        {
            return Path.Combine(AccountDir(account), "matches.ini");
        }

        // Giriş komutlarının yazıldığı MCC betiği; ScriptScheduler "script" eylemiyle çalıştırır.
        public const string LoginScriptFileName = "giris-komutlari.txt";

        public static string AccountLoginScript(AfkAccount account)
        {
            return Path.Combine(AccountDir(account), LoginScriptFileName);
        }
    }

    /// <summary>accounts.json okuma/yazma. Bozuk dosya kullanıcının hesaplarını sessizce silmez, yedeklenir.</summary>
    public static class AfkStore
    {
        private static readonly JsonSerializerSettings JsonSettings = new JsonSerializerSettings
        {
            Formatting = Formatting.Indented,
            NullValueHandling = NullValueHandling.Ignore,
        };

        public static List<AfkAccount> Load()
        {
            try
            {
                if (!File.Exists(AfkPaths.AccountsFile))
                    return new List<AfkAccount>();

                string json = File.ReadAllText(AfkPaths.AccountsFile);
                List<AfkAccount> accounts = JsonConvert.DeserializeObject<List<AfkAccount>>(json, JsonSettings);
                if (accounts == null)
                    return new List<AfkAccount>();

                // Elle düzenlenmiş ya da eski sürümden gelen kayıtlarda Id boş kalmış olabilir.
                foreach (AfkAccount account in accounts)
                {
                    if (string.IsNullOrWhiteSpace(account.Id))
                        account.Id = Guid.NewGuid().ToString("N");
                }

                return accounts;
            }
            catch (Exception)
            {
                BackupCorruptFile();
                return new List<AfkAccount>();
            }
        }

        public static void Save(List<AfkAccount> accounts)
        {
            Directory.CreateDirectory(AfkPaths.Root);

            // Önce geçici dosyaya yazılır: yazma sırasında kapanma hesap listesini boşaltmasın.
            string tempPath = AfkPaths.AccountsFile + ".tmp";
            File.WriteAllText(tempPath, JsonConvert.SerializeObject(accounts, JsonSettings));

            if (File.Exists(AfkPaths.AccountsFile))
                File.Delete(AfkPaths.AccountsFile);
            File.Move(tempPath, AfkPaths.AccountsFile);
        }

        private static void BackupCorruptFile()
        {
            try
            {
                if (!File.Exists(AfkPaths.AccountsFile))
                    return;

                string backup = AfkPaths.AccountsFile + ".bozuk-" + DateTime.Now.ToString("yyyyMMdd-HHmmss");
                File.Copy(AfkPaths.AccountsFile, backup, true);
            }
            catch (Exception)
            {
                // Yedek alınamadıysa da açılışı engellemeyiz.
            }
        }
    }
}
