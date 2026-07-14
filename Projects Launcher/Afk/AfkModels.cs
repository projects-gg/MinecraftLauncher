using System;
using System.Collections.Generic;

namespace Projects_Launcher.Afk
{
    // AFK istemcisinin (Minecraft Console Client) launcher tarafındaki kalıcı modeli.
    // Bu ağaç accounts.json içine yazılır ve her bağlanışta AfkConfigWriter tarafından
    // MCC'nin MinecraftClient.ini dosyasına çevrilir. Tek gerçek kaynak burasıdır:
    // MCC kapanırken ini'yi kendi biçiminde geri yazar, biz de bir sonraki bağlanışta üzerine yazarız.

    public static class AfkDefaults
    {
        public const string ServerHost = "play.projects.gg";

        // Hesap adı doğrulaması ana menüdeki kural ile aynı tutulur.
        public const string NicknamePattern = "^[a-zA-Z0-9_]{3,16}$";
    }

    public class AfkAccount
    {
        public string Id { get; set; } = Guid.NewGuid().ToString("N");

        public string Nickname { get; set; } = string.Empty;

        // Boş bırakılırsa AfkDefaults.ServerHost kullanılır.
        public string ServerHost { get; set; } = string.Empty;

        // 0: belirtilmedi. MCC önce SRV kaydını çözer, bulamazsa 25565'e düşer.
        public int ServerPort { get; set; }

        // Başlatıcı açılır açılmaz bu hesabı bağla.
        public bool AutoStart { get; set; }

        public AntiAfkOptions AntiAfk { get; set; } = new AntiAfkOptions();
        public AutoRelogOptions AutoRelog { get; set; } = new AutoRelogOptions();
        public AutoEatOptions AutoEat { get; set; } = new AutoEatOptions();
        public AutoRespondOptions AutoRespond { get; set; } = new AutoRespondOptions();
        public ChatLogOptions ChatLog { get; set; } = new ChatLogOptions();
        public AlertsOptions Alerts { get; set; } = new AlertsOptions();
        public SchedulerOptions Scheduler { get; set; } = new SchedulerOptions();
        public LoginCommandsOptions LoginCommands { get; set; } = new LoginCommandsOptions();

        public string EffectiveHost
        {
            get { return string.IsNullOrWhiteSpace(ServerHost) ? AfkDefaults.ServerHost : ServerHost.Trim(); }
        }

        public AfkAccount Clone()
        {
            // Ayar penceresi iptal edilirse yapılan değişiklikler kaybolsun diye kopya üzerinde çalışılır.
            return (AfkAccount)Newtonsoft.Json.JsonConvert.DeserializeObject(
                Newtonsoft.Json.JsonConvert.SerializeObject(this), typeof(AfkAccount));
        }
    }

    /// <summary>AFK atılmayı engelleyen periyodik hareket/komut.</summary>
    public class AntiAfkOptions
    {
        public bool Enabled { get; set; } = true;

        // Saniye. İki değer farklıysa MCC aralıktan rastgele seçer (daha doğal görünür).
        public double DelayMin { get; set; } = 60;
        public double DelayMax { get; set; } = 90;

        // AntiAFK her turda bu metni gönderir. Boş bırakılırsa hiçbir şey yazılmaz, yalnızca hareket edilir:
        // sunucuda karşılığı olmayan bir komut ("/ping" her sunucuda yok) her turda hata yanıtı doğurur.
        public string Command { get; set; } = string.Empty;

        public bool UseSneak { get; set; } = true;

        // Açıkken MCC arazi işlemeyi de açmak zorundadır (AfkConfigWriter bunu zorlar).
        public bool UseTerrainHandling { get; set; }

        public int WalkRange { get; set; } = 5;
        public int WalkRetries { get; set; } = 20;
    }

    /// <summary>Bağlantı koptuğunda otomatik yeniden giriş.</summary>
    public class AutoRelogOptions
    {
        public bool Enabled { get; set; } = true;

        public double DelayMin { get; set; } = 5;
        public double DelayMax { get; set; } = 10;

        // -1: sınırsız (MCC bunu int.MaxValue'ya çevirir).
        public int Retries { get; set; } = -1;

        // true: atılma mesajına bakmadan her kopmada yeniden bağlan.
        public bool IgnoreKickMessage { get; set; } = true;

        public List<string> KickMessages { get; set; } = new List<string>
        {
            "Connection has been lost",
            "Server is restarting",
            "Server is full",
            "Too Many people"
        };
    }

    /// <summary>Açlık eşiğin altına inince otomatik yemek yer. Envanter işlemesi gerektirir.</summary>
    public class AutoEatOptions
    {
        public bool Enabled { get; set; }

        // 0-20 arası açlık çubuğu değeri.
        public int Threshold { get; set; } = 6;
    }

    public class AutoRespondOptions
    {
        public bool Enabled { get; set; }
        public bool MatchColors { get; set; }

        // true: hesap bir kurala yanıt gönderdiğinde launcher bildirim sesi çalar.
        public bool SoundEnabled { get; set; }

        public List<AutoRespondMatch> Matches { get; set; } = new List<AutoRespondMatch>();
    }

    /// <summary>matches.ini içindeki tek bir [Match] bloğu.</summary>
    public class AutoRespondMatch
    {
        public string Pattern { get; set; } = string.Empty;

        // true: Pattern bir düzenli ifadedir (matches.ini'de "regex=" satırı olarak yazılır).
        public bool IsRegex { get; set; }

        // "send /home", "log merhaba", "script dosya.cs" gibi MCC eylemleri.
        public string Action { get; set; } = string.Empty;

        // Yalnızca özel mesajla tetiklendiğinde çalışacak eylem (boş bırakılabilir).
        public string PrivateAction { get; set; } = string.Empty;

        // Oyuncu kaynaklı olmayan (sunucu duyurusu) mesajlar için eylem.
        public string OtherAction { get; set; } = string.Empty;

        public bool OwnersOnly { get; set; }

        public int CooldownSeconds { get; set; }
    }

    public class ChatLogOptions
    {
        public bool Enabled { get; set; }
        public bool AddDateTime { get; set; } = true;

        // Canlı konsol görünümünde saatin yanına tarih de yazılsın mı? Dosyayı etkilemez:
        // MCC dosyaya zaten tam tarih-saat yazar (AddDateTime).
        public bool ShowDate { get; set; }

        // MCC'nin beklediği değerler: all, messages, chat
        public string Filter { get; set; } = "messages";
    }

    public class AlertsOptions
    {
        public bool Enabled { get; set; }
        public bool BeepEnabled { get; set; }
        public bool TriggerByWords { get; set; } = true;
        public bool LogToFile { get; set; } = true;

        public List<string> Matches { get; set; } = new List<string>();
        public List<string> Excludes { get; set; } = new List<string>();
    }

    public class SchedulerOptions
    {
        public bool Enabled { get; set; }
        public List<ScheduledTask> Tasks { get; set; } = new List<ScheduledTask>();
    }

    /// <summary>
    /// Oyuna girildiğinde sırayla gönderilecek komut/sohbet satırları. AfkConfigWriter bu listeyi
    /// hesap klasöründeki bir MCC betiğine çevirir ve ScriptScheduler'a girişte tetiklenen
    /// tek bir "script" görevi olarak ekler; böylece komutlar arasında bekleme uygulanabilir.
    /// </summary>
    public class LoginCommandsOptions
    {
        public bool Enabled { get; set; }

        // true: yalnızca ilk girişte; false: yeniden bağlanma dahil her girişte.
        public bool OnlyFirstLogin { get; set; }

        // Komutlar arası bekleme. Sunucuların yazma hızı (spam) engeline takılmamak için.
        public double DelaySeconds { get; set; } = 2;

        public List<string> Commands { get; set; } = new List<string>();
    }

    public class ScheduledTask
    {
        public string Name { get; set; } = "Görev";

        // MCC eylem söz dizimi: "send /kit", "log merhaba", "script dosya.cs"
        public string Action { get; set; } = string.Empty;

        public bool OnLogin { get; set; }
        public bool OnFirstLogin { get; set; }

        public bool IntervalEnabled { get; set; } = true;

        // Saniye cinsinden aralık; MCC her turda bu aralıktan rastgele bekleme seçer.
        public double MinSeconds { get; set; } = 300;
        public double MaxSeconds { get; set; } = 600;
    }
}
