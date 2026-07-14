using System;
using System.Collections.Generic;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Text;

namespace Projects_Launcher.Afk
{
    /// <summary>
    /// AfkAccount modelini Minecraft Console Client'ın beklediği TOML yapılandırmasına çevirir.
    /// Yalnızca launcher'ın yönettiği anahtarlar yazılır; geri kalanı MCC kendi varsayılanlarıyla
    /// doldurur. Dosya her bağlanıştan önce sıfırdan üretilir, bu yüzden MCC'nin çıkışta yaptığı
    /// geri yazma kalıcı değildir.
    /// </summary>
    public static class AfkConfigWriter
    {
        // MCC dosyayı okurken kültürü invariant'a sabitler; biz de yazarken aynısını yapmalıyız,
        // yoksa Türkçe yerelde "60,0" üretilir ve TOML ayrıştırması patlar.
        private static readonly CultureInfo Inv = CultureInfo.InvariantCulture;

        private static readonly UTF8Encoding Utf8NoBom = new UTF8Encoding(false);

        public static void Write(AfkAccount account, string launcherNickname)
        {
            if (account == null) throw new ArgumentNullException("account");

            string dir = AfkPaths.AccountDir(account);
            Directory.CreateDirectory(dir);

            File.WriteAllText(AfkPaths.AccountIni(account), BuildIni(account, launcherNickname), Utf8NoBom);
            File.WriteAllText(AfkPaths.AccountMatchesFile(account), BuildMatchesIni(account), Utf8NoBom);
            File.WriteAllText(AfkPaths.AccountLoginScript(account), BuildLoginScript(account.LoginCommands), Utf8NoBom);
        }

        private static string BuildIni(AfkAccount account, string launcherNickname)
        {
            // AutoEat envanteri okur, AntiAFK'nın yürüme kipi arazi işlemesini gerektirir.
            // Bu bağımlılıkları kullanıcıya sormak yerine burada zorunlu kılıyoruz.
            bool inventoryHandling = account.AutoEat.Enabled;
            bool terrainAndMovements = account.AntiAfk.Enabled && account.AntiAfk.UseTerrainHandling;

            List<string> owners = new List<string>();
            if (!string.IsNullOrWhiteSpace(launcherNickname))
                owners.Add(launcherNickname.Trim());

            StringBuilder sb = new StringBuilder();

            sb.AppendLine("# Projects Launcher tarafından üretildi.");
            sb.AppendLine("# Bu dosya her bağlanışta yeniden yazılır; elle yapılan değişiklikler kaybolur.");
            sb.AppendLine("# Ayarları başlatıcıdaki AFK Hesapları ekranından düzenleyin.");
            sb.AppendLine();

            sb.AppendLine("[Main]");
            sb.AppendLine();
            sb.AppendLine("[Main.General]");
            // Parola "-" MCC'de çevrimdışı (premium olmayan) oturum demektir: hiçbir kimlik doğrulama isteği gitmez.
            sb.AppendLine("Account = { Login = " + Str(account.Nickname) + ", Password = \"-\" }");
            sb.AppendLine("Server = " + ServerTable(account));
            sb.AppendLine();

            sb.AppendLine("[Main.Advanced]");
            sb.AppendLine("EnableSentry = false");
            sb.AppendLine("Language = \"tr_tr\"");
            // Konsol penceresi yok: başlık ve simge değiştirme çağrıları anlamsız.
            sb.AppendLine("ConsoleTitle = \"\"");
            sb.AppendLine("PlayerHeadAsIcon = false");
            sb.AppendLine("ShowGithubStarReminder = false");
            // Etkileşimli olmayan kip: hata anında konsoldan giriş beklemek yerine çıkış kodu ile kapanır.
            sb.AppendLine("ExitOnFailure = true");
            sb.AppendLine("MinecraftVersion = \"auto\"");
            sb.AppendLine("AutoRespawn = true");
            sb.AppendLine("TerrainAndMovements = " + Bool(terrainAndMovements));
            sb.AppendLine("InventoryHandling = " + Bool(inventoryHandling));
            sb.AppendLine("EntityHandling = false");
            // Çevrimdışı hesapta oturum/profil anahtarı yok; diske gereksiz dosya bırakmayalım.
            sb.AppendLine("SessionCache = \"none\"");
            sb.AppendLine("ProfileKeyCache = \"none\"");
            sb.AppendLine("BotOwners = " + Arr(owners));
            sb.AppendLine();

            sb.AppendLine("[Console]");
            sb.AppendLine();
            sb.AppendLine("[Console.General]");
            sb.AppendLine("ConsoleMode = \"classic\"");
            sb.AppendLine("Display_Icon_Banner = false");
            sb.AppendLine();

            sb.AppendLine("[ChatBot]");
            sb.AppendLine();

            AppendAlerts(sb, account.Alerts);
            AppendAntiAfk(sb, account.AntiAfk);
            AppendAutoEat(sb, account.AutoEat);
            AppendAutoRelog(sb, account.AutoRelog);
            AppendAutoRespond(sb, account.AutoRespond);
            AppendChatLog(sb, account.ChatLog);

            // Dizi-tablolar ([[...]]) kendi üst tablolarının basit anahtarlarından sonra gelmek
            // zorunda olduğu için zamanlayıcı en sona bırakılır.
            AppendScheduler(sb, account);

            return sb.ToString();
        }

        private static string ServerTable(AfkAccount account)
        {
            string host = Str(account.EffectiveHost);

            // Port yazılmazsa MCC önce SRV kaydını çözer, bulamazsa 25565 kullanır.
            if (account.ServerPort <= 0 || account.ServerPort > 65535)
                return "{ Host = " + host + " }";

            return "{ Host = " + host + ", Port = " + account.ServerPort.ToString(Inv) + " }";
        }

        private static void AppendAlerts(StringBuilder sb, AlertsOptions o)
        {
            sb.AppendLine("[ChatBot.Alerts]");
            sb.AppendLine("Enabled = " + Bool(o.Enabled));
            sb.AppendLine("Beep_Enabled = " + Bool(o.BeepEnabled));
            sb.AppendLine("Trigger_By_Words = " + Bool(o.TriggerByWords));
            sb.AppendLine("Trigger_By_Rain = false");
            sb.AppendLine("Trigger_By_Thunderstorm = false");
            sb.AppendLine("Log_To_File = " + Bool(o.LogToFile));
            sb.AppendLine("Log_File = \"uyarilar.txt\"");
            sb.AppendLine("Matches = " + Arr(o.Matches));
            sb.AppendLine("Excludes = " + Arr(o.Excludes));
            sb.AppendLine();
        }

        private static void AppendAntiAfk(StringBuilder sb, AntiAfkOptions o)
        {
            sb.AppendLine("[ChatBot.AntiAFK]");
            sb.AppendLine("Enabled = " + Bool(o.Enabled));
            sb.AppendLine("Delay = " + Range(o.DelayMin, o.DelayMax));
            sb.AppendLine("Command = " + Str(o.Command));
            sb.AppendLine("Use_Sneak = " + Bool(o.UseSneak));
            sb.AppendLine("Use_Terrain_Handling = " + Bool(o.UseTerrainHandling));
            sb.AppendLine("Walk_Range = " + o.WalkRange.ToString(Inv));
            sb.AppendLine("Walk_Retries = " + o.WalkRetries.ToString(Inv));
            sb.AppendLine();
        }

        private static void AppendAutoEat(StringBuilder sb, AutoEatOptions o)
        {
            sb.AppendLine("[ChatBot.AutoEat]");
            sb.AppendLine("Enabled = " + Bool(o.Enabled));
            sb.AppendLine("Threshold = " + o.Threshold.ToString(Inv));
            sb.AppendLine();
        }

        private static void AppendAutoRelog(StringBuilder sb, AutoRelogOptions o)
        {
            sb.AppendLine("[ChatBot.AutoRelog]");
            sb.AppendLine("Enabled = " + Bool(o.Enabled));
            sb.AppendLine("Delay = " + Range(o.DelayMin, o.DelayMax));
            sb.AppendLine("Retries = " + o.Retries.ToString(Inv));
            sb.AppendLine("Ignore_Kick_Message = " + Bool(o.IgnoreKickMessage));
            sb.AppendLine("Kick_Messages = " + Arr(o.KickMessages));
            sb.AppendLine();
        }

        private static void AppendAutoRespond(StringBuilder sb, AutoRespondOptions o)
        {
            sb.AppendLine("[ChatBot.AutoRespond]");
            // Eşleşme listesi boşken bot yüklenirse MCC her sohbette boş dosyayı okumaya çalışır.
            sb.AppendLine("Enabled = " + Bool(o.Enabled && o.Matches.Count > 0));
            sb.AppendLine("Matches_File = \"matches.ini\"");
            sb.AppendLine("Match_Colors = " + Bool(o.MatchColors));
            sb.AppendLine();
        }

        private static void AppendChatLog(StringBuilder sb, ChatLogOptions o)
        {
            sb.AppendLine("[ChatBot.ChatLog]");
            sb.AppendLine("Enabled = " + Bool(o.Enabled));
            sb.AppendLine("Add_DateTime = " + Bool(o.AddDateTime));
            sb.AppendLine("Log_File = \"sohbet.txt\"");
            sb.AppendLine("Filter = " + Str(o.Filter));
            sb.AppendLine();
        }

        private static void AppendScheduler(StringBuilder sb, AfkAccount account)
        {
            SchedulerOptions o = account.Scheduler;

            List<ScheduledTask> tasks = o.Enabled
                ? o.Tasks.Where(t => !string.IsNullOrWhiteSpace(t.Action)).ToList()
                : new List<ScheduledTask>();

            // Giriş komutları da ScriptScheduler ile çalışır: girişte tetiklenen tek bir betik görevi.
            // Kullanıcının kendi zamanlayıcısı kapalı olsa bile bu görev tek başına bot'u açar.
            if (HasLoginCommands(account.LoginCommands))
            {
                tasks.Add(new ScheduledTask
                {
                    Name = "Giriş Komutları",
                    Action = "script " + AfkPaths.LoginScriptFileName,
                    OnFirstLogin = account.LoginCommands.OnlyFirstLogin,
                    OnLogin = !account.LoginCommands.OnlyFirstLogin,
                    IntervalEnabled = false,
                });
            }

            sb.AppendLine("[ChatBot.ScriptScheduler]");
            sb.AppendLine("Enabled = " + Bool(tasks.Count > 0));

            if (tasks.Count == 0)
            {
                // Boş bırakılırsa MCC kendi örnek görevlerini ("send /hello") yükler. Açıkça boşaltıyoruz.
                sb.AppendLine("TaskList = [ ]");
                sb.AppendLine();
                return;
            }

            sb.AppendLine();

            foreach (ScheduledTask task in tasks)
            {
                sb.AppendLine("[[ChatBot.ScriptScheduler.TaskList]]");
                sb.AppendLine("Task_Name = " + Str(string.IsNullOrWhiteSpace(task.Name) ? "Görev" : task.Name));
                sb.AppendLine("Trigger_On_First_Login = " + Bool(task.OnFirstLogin));
                sb.AppendLine("Trigger_On_Login = " + Bool(task.OnLogin));
                sb.AppendLine("Trigger_On_Times = { Enable = false, Times = [ ] }");
                sb.AppendLine("Trigger_On_Interval = { Enable = " + Bool(task.IntervalEnabled) +
                              ", MinTime = " + Num(task.MinSeconds) +
                              ", MaxTime = " + Num(task.MaxSeconds) + " }");
                sb.AppendLine("Action = " + Str(task.Action));
                sb.AppendLine();
            }
        }

        private static bool HasLoginCommands(LoginCommandsOptions o)
        {
            return o != null && o.Enabled && o.Commands != null &&
                   o.Commands.Any(c => !string.IsNullOrWhiteSpace(c));
        }

        // MCC'nin klasik betik yorumlayıcısı "wait N"i ham istemci tiki sayar ve Update saniyede
        // ClientTicksPerSecond kez çağrılır (20). Yani saniye başına 20 tik: wait 20 = 1 sn.
        private const int ScriptTicksPerSecond = 20;

        /// <summary>
        /// Giriş komutlarını MCC betiğine çevirir. Betikte her satır bir MCC iç komutudur;
        /// sohbete/sunucuya yazmak "send" ile yapılır, "wait N" ise N tik (N/20 sn) bekler.
        /// </summary>
        private static string BuildLoginScript(LoginCommandsOptions o)
        {
            StringBuilder sb = new StringBuilder();
            sb.AppendLine("# Projects Launcher tarafından üretildi. Elle yapılan değişiklikler kaybolur.");

            if (!HasLoginCommands(o))
                return sb.ToString();

            List<string> commands = o.Commands
                .Select(c => c == null ? string.Empty : c.Trim())
                .Where(c => c.Length > 0)
                .ToList();

            int waitTicks = (int)Math.Round(Math.Max(0, o.DelaySeconds) * ScriptTicksPerSecond);

            // Girişin hemen ardından gelen komutları sunucular çoğu kez yutar: oyuncu daha lobide
            // işlenirken sohbet kabul edilmez. İlk komuttan önce de aynı süre kadar bekleniyor.
            if (waitTicks > 0)
                sb.AppendLine("wait " + waitTicks.ToString(Inv));

            for (int i = 0; i < commands.Count; i++)
            {
                sb.AppendLine("send " + commands[i]);

                if (waitTicks > 0 && i < commands.Count - 1)
                    sb.AppendLine("wait " + waitTicks.ToString(Inv));
            }

            return sb.ToString();
        }

        /// <summary>
        /// AutoRespond'un okuduğu matches.ini. TOML değil, MCC'nin kendi INIFile biçimidir:
        /// her blok [Match] başlığı ve ardından anahtar=değer satırlarıyla tanımlanır.
        /// </summary>
        private static string BuildMatchesIni(AfkAccount account)
        {
            StringBuilder sb = new StringBuilder();
            sb.AppendLine("# Projects Launcher tarafından üretildi. Elle yapılan değişiklikler kaybolur.");
            sb.AppendLine();

            foreach (AutoRespondMatch match in account.AutoRespond.Matches)
            {
                if (string.IsNullOrWhiteSpace(match.Pattern))
                    continue;

                bool hasAction = !string.IsNullOrWhiteSpace(match.Action)
                                 || !string.IsNullOrWhiteSpace(match.PrivateAction)
                                 || !string.IsNullOrWhiteSpace(match.OtherAction);
                if (!hasAction)
                    continue;

                sb.AppendLine("[Match]");
                sb.AppendLine((match.IsRegex ? "regex=" : "match=") + match.Pattern.Trim());

                if (!string.IsNullOrWhiteSpace(match.Action))
                    sb.AppendLine("action=" + match.Action.Trim());
                if (!string.IsNullOrWhiteSpace(match.PrivateAction))
                    sb.AppendLine("actionprivate=" + match.PrivateAction.Trim());
                if (!string.IsNullOrWhiteSpace(match.OtherAction))
                    sb.AppendLine("actionother=" + match.OtherAction.Trim());

                sb.AppendLine("ownersonly=" + (match.OwnersOnly ? "true" : "false"));
                sb.AppendLine("cooldown=" + Math.Max(0, match.CooldownSeconds).ToString(Inv));
                sb.AppendLine();
            }

            return sb.ToString();
        }

        // --- TOML ilkel yazıcıları ---

        private static string Bool(bool value)
        {
            return value ? "true" : "false";
        }

        /// <summary>TOML float: MCC'deki Range/MinTime alanları tamsayı kabul etmez, ondalık nokta şarttır.</summary>
        private static string Num(double value)
        {
            return value.ToString("0.0##", Inv);
        }

        private static string Range(double min, double max)
        {
            return "{ min = " + Num(min) + ", max = " + Num(max) + " }";
        }

        private static string Str(string value)
        {
            if (value == null)
                value = string.Empty;

            StringBuilder sb = new StringBuilder(value.Length + 2);
            sb.Append('"');

            foreach (char c in value)
            {
                switch (c)
                {
                    case '\\': sb.Append("\\\\"); break;
                    case '"': sb.Append("\\\""); break;
                    case '\n': sb.Append("\\n"); break;
                    case '\r': sb.Append("\\r"); break;
                    case '\t': sb.Append("\\t"); break;
                    default:
                        // TOML temel dizgilerinde kontrol karakterleri kaçışsız yer alamaz.
                        if (c < 0x20)
                            sb.Append("\\u").Append(((int)c).ToString("X4", Inv));
                        else
                            sb.Append(c);
                        break;
                }
            }

            sb.Append('"');
            return sb.ToString();
        }

        private static string Arr(IEnumerable<string> items)
        {
            List<string> values = items == null
                ? new List<string>()
                : items.Where(i => !string.IsNullOrWhiteSpace(i)).Select(i => Str(i.Trim())).ToList();

            if (values.Count == 0)
                return "[ ]";

            return "[ " + string.Join(", ", values.ToArray()) + " ]";
        }
    }
}
