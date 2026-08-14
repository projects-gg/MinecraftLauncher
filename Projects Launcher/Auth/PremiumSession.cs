using CmlLib.Core.Auth;
using Newtonsoft.Json;
using System;
using System.IO;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace Projects_Launcher.Auth
{
    /// <summary>Giriş yapılmış premium (satın alınmış) Minecraft hesabı.</summary>
    public sealed class PremiumAccount
    {
        /// <summary>Oyun içi görünen ad. Premium hesaplarda kullanıcı tarafından değiştirilemez.</summary>
        public string Name { get; set; }

        /// <summary>Minecraft profil kimliği (tiresiz 32 karakter).</summary>
        public string Uuid { get; set; }

        /// <summary>Xbox kullanıcı kimliği; oyun bazı sürümlerde başlatma bağımsız değişkeni olarak ister.</summary>
        public string Xuid { get; set; }

        /// <summary>Oyunun sunucuya kimlik doğrulaması için kullandığı anahtar (yaklaşık 24 saat geçerli).</summary>
        public string AccessToken { get; set; }

        public DateTime ExpiresAtUtc { get; set; }

        /// <summary>Kullanıcıya tekrar sormadan yeni anahtar almayı sağlayan Microsoft yenileme anahtarı.</summary>
        public string RefreshToken { get; set; }

        /// <summary>
        /// Anahtar dolmuş ya da dolmak üzere mi? Oyun başlatılırken süresi bitmek üzere olan bir
        /// anahtarla girmek "oturum geçersiz" hatasına yol açtığından 10 dakikalık pay bırakılır.
        /// </summary>
        [JsonIgnore]
        public bool NeedsRefresh
        {
            get { return DateTime.UtcNow >= ExpiresAtUtc - TimeSpan.FromMinutes(10); }
        }
    }

    /// <summary>
    /// Premium oturumun diskteki hâli ve tazelenmesi.
    /// <para>
    /// Yenileme anahtarı bir parola kadar değerlidir: dosya Windows'un DPAPI'siyle yalnızca giriş
    /// yapmış kullanıcı çözebilecek şekilde şifrelenir. Kopyalanan dosya başka kullanıcıda ya da
    /// başka bilgisayarda çözülemez, o durumda oturum yok sayılır.
    /// </para>
    /// </summary>
    public static class PremiumSession
    {
        private static readonly object Gate = new object();
        private static readonly SemaphoreSlim RefreshGate = new SemaphoreSlim(1, 1);

        private static PremiumAccount _current;
        private static bool _loaded;

        /// <summary>
        /// Premium giriş yolu bu sürümde açık mı? (<c>enablePremiumLoginMethod</c> ayarı.)
        /// Kapalıyken kayıtlı bir oturum olsa bile yok sayılır: giriş ekranındaki premium butonu,
        /// ana menüdeki rozet ve çevrimiçi mod başlatma tümüyle devre dışı kalır.
        /// </summary>
        public static bool IsEnabled
        {
            get { return Properties.Settings.Default.enablePremiumLoginMethod; }
        }

        /// <summary>Kayıtlı premium hesap; giriş yapılmamışsa ya da özellik kapalıysa <c>null</c>.</summary>
        public static PremiumAccount Current
        {
            get
            {
                if (!IsEnabled)
                    return null;

                EnsureLoaded();
                return _current;
            }
        }

        public static bool IsActive
        {
            get { return Current != null; }
        }

        /// <summary>Premium oturum açıksa oyun içi ad, değilse <c>null</c>.</summary>
        public static string ActiveName
        {
            get
            {
                PremiumAccount account = Current;
                return account != null ? account.Name : null;
            }
        }

        private static string StorePath
        {
            get
            {
                return Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                    ".projects",
                    "premium.dat");
            }
        }

        /// <summary>Yeni açılan oturumu belleğe ve diske yazar.</summary>
        public static void Save(PremiumAccount account)
        {
            if (account == null)
                throw new ArgumentNullException("account");

            lock (Gate)
            {
                _current = account;
                _loaded = true;
                WriteToDisk(account);
            }
        }

        /// <summary>Premium oturumu kapatır ve diskteki kaydı siler.</summary>
        public static void SignOut()
        {
            lock (Gate)
            {
                _current = null;
                _loaded = true;

                try
                {
                    if (File.Exists(StorePath))
                        File.Delete(StorePath);
                }
                catch (Exception)
                {
                    // Dosya kilitliyse bir sonraki açılışta çözülemeyip yok sayılır.
                }
            }
        }

        /// <summary>
        /// Oyun başlatılmadan önce çağrılır: anahtarın süresi dolmuşsa kullanıcıya sormadan tazeler.
        /// Yenileme anahtarı da geçersizse oturum temizlenir ve <see cref="PremiumAuthException"/>
        /// fırlatılır (<see cref="PremiumAuthException.RequiresSignIn"/> ile).
        /// </summary>
        public static async Task<PremiumAccount> EnsureValidAsync(CancellationToken cancellationToken)
        {
            PremiumAccount account = Current;
            if (account == null)
                return null;

            if (!account.NeedsRefresh)
                return account;

            // Aynı anda birden çok yerden (oyun başlatma + arka plan tazeleme) çağrılabilir;
            // tek seferde yalnızca bir yenileme isteği gitsin.
            await RefreshGate.WaitAsync(cancellationToken).ConfigureAwait(false);

            try
            {
                account = Current;
                if (account == null)
                    return null;

                if (!account.NeedsRefresh)
                    return account;

                try
                {
                    PremiumAccount refreshed = await MicrosoftAuth
                        .RefreshAsync(account.RefreshToken, null, cancellationToken)
                        .ConfigureAwait(false);

                    // Microsoft yenileme sırasında yeni bir yenileme anahtarı vermeyebilir;
                    // o durumda eldeki anahtar korunur, yoksa oturum bir sonraki sefere düşerdi.
                    if (string.IsNullOrEmpty(refreshed.RefreshToken))
                        refreshed.RefreshToken = account.RefreshToken;

                    Save(refreshed);
                    return refreshed;
                }
                catch (PremiumAuthException exception)
                {
                    if (exception.RequiresSignIn)
                        SignOut();

                    throw;
                }
            }
            finally
            {
                RefreshGate.Release();
            }
        }

        /// <summary>Premium hesabı oyunun beklediği oturum nesnesine çevirir (çevrimiçi mod).</summary>
        public static MSession ToMSession(PremiumAccount account)
        {
            if (account == null)
                throw new ArgumentNullException("account");

            return new MSession(account.Name, account.AccessToken, account.Uuid)
            {
                // Microsoft hesabıyla açılan oturumlar oyuna "msa" olarak bildirilir;
                // eski "mojang" değeri güncel sürümlerde kimlik doğrulamasını bozar.
                UserType = "msa",
                Xuid = account.Xuid,
            };
        }

        // --- Disk ---

        private static void EnsureLoaded()
        {
            lock (Gate)
            {
                if (_loaded)
                    return;

                _loaded = true;
                _current = ReadFromDisk();
            }
        }

        private static PremiumAccount ReadFromDisk()
        {
            try
            {
                string path = StorePath;
                if (!File.Exists(path))
                    return null;

                byte[] encrypted = Convert.FromBase64String(File.ReadAllText(path, Encoding.ASCII));
                byte[] plain = ProtectedData.Unprotect(encrypted, null, DataProtectionScope.CurrentUser);

                PremiumAccount account = JsonConvert.DeserializeObject<PremiumAccount>(Encoding.UTF8.GetString(plain));

                // Eksik alanlı kayıt oyunu başlatamaz; yok sayılır ki kullanıcı temiz bir giriş yapsın.
                if (account == null ||
                    string.IsNullOrEmpty(account.Name) ||
                    string.IsNullOrEmpty(account.Uuid) ||
                    string.IsNullOrEmpty(account.RefreshToken))
                {
                    return null;
                }

                return account;
            }
            catch (Exception)
            {
                // Bozuk, başka kullanıcıya ait ya da başka bilgisayardan kopyalanmış dosya:
                // çözülemez, premium giriş yapılmamış sayılır.
                return null;
            }
        }

        private static void WriteToDisk(PremiumAccount account)
        {
            try
            {
                string path = StorePath;
                Directory.CreateDirectory(Path.GetDirectoryName(path));

                byte[] plain = Encoding.UTF8.GetBytes(JsonConvert.SerializeObject(account));
                byte[] encrypted = ProtectedData.Protect(plain, null, DataProtectionScope.CurrentUser);

                File.WriteAllText(path, Convert.ToBase64String(encrypted), Encoding.ASCII);
            }
            catch (Exception)
            {
                // Yazılamadıysa oturum bu açılış boyunca bellekte yaşamaya devam eder.
            }
        }
    }
}
