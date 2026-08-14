using Newtonsoft.Json.Linq;
using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Globalization;
using System.IO;
using System.Net;
using System.Net.Http;
using System.Net.Http.Headers;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using System.Threading;
using System.Threading.Tasks;

namespace Projects_Launcher.Auth
{
    /// <summary>Premium giriş akışının kullanıcıya gösterilen aşamaları.</summary>
    public enum PremiumAuthStage
    {
        /// <summary>Varsayılan tarayıcıda Microsoft giriş sayfası açıldı, kullanıcı bekleniyor.</summary>
        WaitingBrowser,

        /// <summary>Tarayıcıdan dönen yetki kodu erişim anahtarına çevriliyor.</summary>
        ExchangingCode,

        /// <summary>Xbox Live oturumu açılıyor.</summary>
        XboxLive,

        /// <summary>Minecraft oturumu alınıyor (Xbox güvenlik anahtarı doğrulanıyor).</summary>
        Minecraft,

        /// <summary>Oyun lisansı ve profil bilgisi okunuyor.</summary>
        Profile,
    }

    /// <summary>
    /// Kullanıcıya olduğu gibi gösterilebilen giriş hatası: mesajları teknik ayrıntı içermez,
    /// her biri kullanıcının ne yapması gerektiğini söyler.
    /// </summary>
    public sealed class PremiumAuthException : Exception
    {
        public PremiumAuthException(string message) : base(message)
        {
        }

        public PremiumAuthException(string message, Exception innerException) : base(message, innerException)
        {
        }

        /// <summary>
        /// Kayıtlı oturum artık kullanılamaz (yenileme anahtarı geçersiz): çağıran taraf kayıtlı
        /// hesabı temizleyip kullanıcıyı yeniden giriş yapmaya yönlendirmelidir. Geçici ağ/servis
        /// hatalarında <c>false</c> kalır ki tek bir bağlantı kesintisi hesabı düşürmesin.
        /// </summary>
        public bool RequiresSignIn { get; set; }

        /// <summary>
        /// Kullanıcıya gösterilen kısa hata kodu (ör. "MC-403"). Destek talebinde paylaşıldığında
        /// sorunun hangi adımda ve neden oluştuğu tek bakışta anlaşılır.
        /// </summary>
        public string ErrorCode { get; set; }

        /// <summary>
        /// "Kodu Kopyala" ile panoya alınan ayrıntılı teşhis metni. Arayüzde gösterilmez;
        /// yalnızca kullanıcı kendisi paylaşmayı seçerse dışarı çıkar.
        /// </summary>
        public string Diagnostics { get; set; }
    }

    /// <summary>
    /// Microsoft hesabıyla Minecraft (premium) oturumu açar.
    /// <para>
    /// Zincir: Microsoft OAuth 2.0 (PKCE) → Xbox Live → XSTS → Minecraft Services → oyuncu profili.
    /// Yetki kodu, varsayılan tarayıcı yerel makinedeki geçici bir dinleyiciye yönlendirilerek alınır
    /// (RFC 8252 "loopback" deseni); böylece launcher içinde gömülü tarayıcı taşımak gerekmez ve
    /// kullanıcı parolasını yalnızca Microsoft'un kendi sayfasına girer.
    /// </para>
    /// </summary>
    public static class MicrosoftAuth
    {
        /// <summary>
        /// Azure portalında "Personal Microsoft accounts only" olarak ve
        /// "http://localhost" yönlendirmesiyle kayıtlı uygulamanın kimliği.
        /// </summary>
        public const string ClientId = "24f2a0f7-5640-4f4a-847b-ac8f31e4bb24";

        // Minecraft için gereken en dar izin kümesi. offline_access olmadan her açılışta
        // yeniden giriş yapmak gerekirdi (yenileme anahtarı verilmez).
        private const string Scope = "XboxLive.signin offline_access";

        private const string AuthorizeEndpoint = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize";
        private const string TokenEndpoint = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token";
        private const string XboxLiveEndpoint = "https://user.auth.xboxlive.com/user/authenticate";
        private const string XstsEndpoint = "https://xsts.auth.xboxlive.com/xsts/authorize";
        private const string MinecraftLoginEndpoint = "https://api.minecraftservices.com/authentication/login_with_xbox";
        private const string MinecraftProfileEndpoint = "https://api.minecraftservices.com/minecraft/profile";

        private static readonly HttpClient Http = new HttpClient { Timeout = TimeSpan.FromSeconds(30) };

        static MicrosoftAuth()
        {
            // Xbox Live ve Minecraft uçları TLS 1.2 altını reddeder; .NET Framework varsayılanı
            // işletim sistemi ayarına bağlı olduğundan burada açıkça sabitlenir.
            ServicePointManager.SecurityProtocol |= SecurityProtocolType.Tls12;

            Http.DefaultRequestHeaders.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));

            // .NET her POST'a "Expect: 100-continue" ön anlaşması ekler; bazı uçlar bunu
            // reddediyor. Yalnızca burada kullanılan adresler için kapatılır, uygulamanın
            // genel ağ ayarına dokunulmaz.
            foreach (string endpoint in new[]
            {
                TokenEndpoint, XboxLiveEndpoint, XstsEndpoint, MinecraftLoginEndpoint, MinecraftProfileEndpoint,
            })
            {
                ServicePointManager.FindServicePoint(new Uri(endpoint)).Expect100Continue = false;
            }

            try
            {
                // Minecraft Services, istemcisini tanıtmayan (User-Agent göndermeyen) istekleri
                // reddedebiliyor. HttpClient varsayılan olarak bu başlığı hiç göndermez.
                Http.DefaultRequestHeaders.UserAgent.ParseAdd(
                    "ProjectsLauncher/" + Properties.Settings.Default.currentVersion);
            }
            catch (FormatException)
            {
                Http.DefaultRequestHeaders.UserAgent.ParseAdd("ProjectsLauncher");
            }
        }

        /// <summary>Uygulama kimliği doldurulmuş mu? Boşsa premium giriş başlatılamaz.</summary>
        public static bool IsConfigured
        {
            get
            {
                Guid parsed;
                return Guid.TryParse(ClientId, out parsed) && parsed != Guid.Empty;
            }
        }

        /// <summary>
        /// Tarayıcı üzerinden yeni bir premium oturum açar. Kullanıcı pencereyi kapatır ya da
        /// vazgeçerse <see cref="OperationCanceledException"/> fırlatır.
        /// </summary>
        public static async Task<PremiumAccount> SignInAsync(IProgress<PremiumAuthStage> progress, CancellationToken cancellationToken)
        {
            if (!IsConfigured)
                throw Fail("Premium giriş bu başlatıcı sürümünde yapılandırılmamış.", "AYAR", "yapilandirma", "client_id bos");

            string codeVerifier = CreateRandomToken(32);
            string codeChallenge = CreateCodeChallenge(codeVerifier);
            string expectedState = CreateRandomToken(16);

            using (LoopbackServer loopback = new LoopbackServer())
            {
                Dictionary<string, string> authorizeParameters = new Dictionary<string, string>
                {
                    { "client_id", ClientId },
                    { "response_type", "code" },
                    { "redirect_uri", loopback.RedirectUri },
                    { "response_mode", "query" },
                    { "scope", Scope },
                    { "state", expectedState },
                    { "code_challenge", codeChallenge },
                    { "code_challenge_method", "S256" },
                    // Bilgisayarda birden çok Microsoft hesabı varsa kullanıcı hangisiyle
                    // gireceğini seçebilsin (aksi halde tarayıcıdaki oturum sessizce kullanılır).
                    { "prompt", "select_account" },
                };

                // Adım göstergesi tarayıcı açılmadan önce yanar: kullanıcı ne beklendiğini görsün.
                Report(progress, PremiumAuthStage.WaitingBrowser);
                OpenInBrowser(AuthorizeEndpoint + "?" + BuildQueryString(authorizeParameters));

                Dictionary<string, string> callback = await loopback.WaitForCallbackAsync(cancellationToken).ConfigureAwait(false);

                string error;
                if (callback.TryGetValue("error", out error))
                    throw Fail(DescribeAuthorizeError(error), "MSA-" + error, "yetkilendirme", "authorize error=" + error);

                string state;
                if (!callback.TryGetValue("state", out state) || state != expectedState)
                    throw Fail("Giriş yanıtı doğrulanamadı. Lütfen tekrar deneyin.", "MSA-STATE", "yetkilendirme", "state uyusmadi");

                string code;
                if (!callback.TryGetValue("code", out code) || string.IsNullOrEmpty(code))
                    throw Fail("Microsoft'tan giriş onayı alınamadı. Lütfen tekrar deneyin.", "MSA-NOCODE", "yetkilendirme", "code parametresi yok");

                Report(progress, PremiumAuthStage.ExchangingCode);

                Dictionary<string, string> tokenRequest = new Dictionary<string, string>
                {
                    { "client_id", ClientId },
                    { "grant_type", "authorization_code" },
                    { "code", code },
                    { "redirect_uri", loopback.RedirectUri },
                    { "code_verifier", codeVerifier },
                    { "scope", Scope },
                };

                JObject token = await RequestMicrosoftTokenAsync(tokenRequest, cancellationToken).ConfigureAwait(false);
                return await CompleteMinecraftChainAsync(token, progress, cancellationToken).ConfigureAwait(false);
            }
        }

        /// <summary>
        /// Kayıtlı yenileme anahtarıyla oturumu kullanıcıya sormadan tazeler.
        /// Anahtar geçersizse (parola değişimi, izin iptali) <see cref="PremiumAuthException"/> fırlatır.
        /// </summary>
        public static async Task<PremiumAccount> RefreshAsync(string refreshToken, IProgress<PremiumAuthStage> progress, CancellationToken cancellationToken)
        {
            if (string.IsNullOrEmpty(refreshToken))
                throw Fail("Kayıtlı premium oturum bulunamadı.", "MSA-NOREFRESH", "yenileme", "refresh_token bos");

            Dictionary<string, string> tokenRequest = new Dictionary<string, string>
            {
                { "client_id", ClientId },
                { "grant_type", "refresh_token" },
                { "refresh_token", refreshToken },
                { "scope", Scope },
            };

            JObject token = await RequestMicrosoftTokenAsync(tokenRequest, cancellationToken).ConfigureAwait(false);
            return await CompleteMinecraftChainAsync(token, progress, cancellationToken).ConfigureAwait(false);
        }

        // --- Microsoft → Xbox Live → XSTS → Minecraft zinciri ---

        private static async Task<PremiumAccount> CompleteMinecraftChainAsync(JObject microsoftToken, IProgress<PremiumAuthStage> progress, CancellationToken cancellationToken)
        {
            string microsoftAccessToken = (string)microsoftToken["access_token"];
            string refreshToken = (string)microsoftToken["refresh_token"];

            if (string.IsNullOrEmpty(microsoftAccessToken))
                throw Fail("Microsoft hesabı doğrulanamadı. Lütfen tekrar deneyin.", "MSA-NOTOKEN", "anahtar", "access_token bos");

            Report(progress, PremiumAuthStage.XboxLive);
            XboxToken xbox = await AuthenticateXboxLiveAsync(microsoftAccessToken, cancellationToken).ConfigureAwait(false);
            XboxToken xsts = await AuthorizeXstsAsync(xbox.Token, cancellationToken).ConfigureAwait(false);

            Report(progress, PremiumAuthStage.Minecraft);
            JObject minecraftToken = await LoginWithXboxAsync(xsts.UserHash, xsts.Token, cancellationToken).ConfigureAwait(false);

            string accessToken = (string)minecraftToken["access_token"];
            if (string.IsNullOrEmpty(accessToken))
                throw Fail("Minecraft oturumu açılamadı. Lütfen tekrar deneyin.", "MC-NOTOKEN", "minecraft-oturum", "access_token bos");

            int expiresInSeconds = (int?)minecraftToken["expires_in"] ?? 0;
            if (expiresInSeconds <= 0)
                expiresInSeconds = 86400; // Minecraft anahtarı yaklaşık 24 saat geçerlidir

            Report(progress, PremiumAuthStage.Profile);
            JObject profile = await FetchProfileAsync(accessToken, cancellationToken).ConfigureAwait(false);

            string uuid = (string)profile["id"];
            string name = (string)profile["name"];

            if (string.IsNullOrEmpty(uuid) || string.IsNullOrEmpty(name))
                throw Fail("Minecraft profili okunamadı. Lütfen tekrar deneyin.", "MC-PROFILVERI", "profil", "id/name alanlari bos");

            return new PremiumAccount
            {
                Name = name,
                Uuid = uuid,
                Xuid = xsts.Xuid,
                AccessToken = accessToken,
                ExpiresAtUtc = DateTime.UtcNow.AddSeconds(expiresInSeconds),
                RefreshToken = refreshToken,
            };
        }

        private static async Task<XboxToken> AuthenticateXboxLiveAsync(string microsoftAccessToken, CancellationToken cancellationToken)
        {
            JObject body = new JObject
            {
                ["Properties"] = new JObject
                {
                    ["AuthMethod"] = "RPS",
                    ["SiteName"] = "user.auth.xboxlive.com",
                    // "d=" öneki, anahtarın Azure (consumers) ucundan geldiğini belirtir.
                    ["RpsTicket"] = "d=" + microsoftAccessToken,
                },
                ["RelyingParty"] = "http://auth.xboxlive.com",
                ["TokenType"] = "JWT",
            };

            HttpResponse response = await PostJsonAsync(XboxLiveEndpoint, body, cancellationToken).ConfigureAwait(false);

            if (!response.IsSuccess)
                throw Fail("Xbox Live oturumu açılamadı. Microsoft hesabınızda Xbox profili olduğundan emin olun.", "XBL-" + (int)response.StatusCode, "xbox-live", response);

            return ReadXboxToken(response.Body);
        }

        private static async Task<XboxToken> AuthorizeXstsAsync(string xboxLiveToken, CancellationToken cancellationToken)
        {
            JObject body = new JObject
            {
                ["Properties"] = new JObject
                {
                    ["SandboxId"] = "RETAIL",
                    ["UserTokens"] = new JArray(xboxLiveToken),
                },
                ["RelyingParty"] = "rp://api.minecraftservices.com/",
                ["TokenType"] = "JWT",
            };

            HttpResponse response = await PostJsonAsync(XstsEndpoint, body, cancellationToken).ConfigureAwait(false);

            if (!response.IsSuccess)
                throw Fail(DescribeXstsError(response.Body), "XSTS-" + (ReadErrorTag(response.Body, "XErr") ?? ((int)response.StatusCode).ToString(CultureInfo.InvariantCulture)), "xsts", response);

            return ReadXboxToken(response.Body);
        }

        private static async Task<JObject> LoginWithXboxAsync(string userHash, string xstsToken, CancellationToken cancellationToken)
        {
            JObject body = new JObject
            {
                ["identityToken"] = "XBL3.0 x=" + userHash + ";" + xstsToken,
            };

            HttpResponse response = await PostJsonAsync(MinecraftLoginEndpoint, body, cancellationToken).ConfigureAwait(false);

            if (!response.IsSuccess)
                throw CreateMinecraftLoginException(response);

            return ParseJson(response.Body);
        }

        private static async Task<JObject> FetchProfileAsync(string minecraftAccessToken, CancellationToken cancellationToken)
        {
            using (HttpRequestMessage request = new HttpRequestMessage(HttpMethod.Get, MinecraftProfileEndpoint))
            {
                request.Headers.Authorization = new AuthenticationHeaderValue("Bearer", minecraftAccessToken);

                HttpResponse response = await SendAsync(request, cancellationToken).ConfigureAwait(false);

                // Lisans yoksa Minecraft Services profil döndürmez. Kullanıcının anlaması gereken
                // tek hata budur: hesapta oyun yok.
                if (response.StatusCode == HttpStatusCode.NotFound)
                {
                    PremiumAuthException missingLicense = Fail(
                        "Bu Microsoft hesabında Minecraft: Java Edition bulunamadı.\n\n" +
                        "Premium giriş için oyunun bu hesapla satın alınmış olması gerekir.",
                        "MC-LISANSYOK", "profil", response);

                    // Lisans yoksa kayıtlı oturumu tutmanın anlamı yok; hesap temizlenir.
                    missingLicense.RequiresSignIn = true;
                    throw missingLicense;
                }

                if (!response.IsSuccess)
                    throw Fail("Minecraft profili alınamadı. Lütfen tekrar deneyin.", "MC-PROFIL-" + (int)response.StatusCode, "profil", response);

                return ParseJson(response.Body);
            }
        }

        // --- HTTP yardımcıları ---

        private static async Task<JObject> RequestMicrosoftTokenAsync(Dictionary<string, string> parameters, CancellationToken cancellationToken)
        {
            using (HttpRequestMessage request = new HttpRequestMessage(HttpMethod.Post, TokenEndpoint))
            {
                request.Content = new FormUrlEncodedContent(parameters);

                HttpResponse response = await SendAsync(request, cancellationToken).ConfigureAwait(false);

                if (!response.IsSuccess)
                    throw CreateTokenException(response.Body);

                return ParseJson(response.Body);
            }
        }

        private static async Task<HttpResponse> PostJsonAsync(string url, JObject body, CancellationToken cancellationToken)
        {
            using (HttpRequestMessage request = new HttpRequestMessage(HttpMethod.Post, url))
            {
                request.Content = new StringContent(body.ToString(Newtonsoft.Json.Formatting.None), Encoding.UTF8, "application/json");
                request.Headers.Accept.Add(new MediaTypeWithQualityHeaderValue("application/json"));

                return await SendAsync(request, cancellationToken).ConfigureAwait(false);
            }
        }

        private static async Task<HttpResponse> SendAsync(HttpRequestMessage request, CancellationToken cancellationToken)
        {
            try
            {
                using (HttpResponseMessage response = await Http.SendAsync(request, cancellationToken).ConfigureAwait(false))
                {
                    string content = await response.Content.ReadAsStringAsync().ConfigureAwait(false);

                    if (!response.IsSuccessStatusCode)
                        LogFailure(request.RequestUri, response.StatusCode, content);

                    return new HttpResponse(response.StatusCode, response.IsSuccessStatusCode, content);
                }
            }
            catch (OperationCanceledException) when (cancellationToken.IsCancellationRequested)
            {
                throw;
            }
            catch (Exception exception)
            {
                throw Fail(
                    "Sunuculara ulaşılamadı.\nİnternet bağlantınızı kontrol edip tekrar deneyin.",
                    "AG-BAGLANTI",
                    request.RequestUri.Host,
                    exception.GetType().Name + ": " + exception.Message);
            }
        }

        private static JObject ParseJson(string content)
        {
            try
            {
                return JObject.Parse(content);
            }
            catch (Exception exception)
            {
                throw Fail("Sunucudan beklenmedik bir yanıt geldi. Lütfen tekrar deneyin.",
                    "VERI-BICIM", "yanit-cozumleme", exception.Message);
            }
        }

        private static XboxToken ReadXboxToken(string content)
        {
            JObject payload = ParseJson(content);

            string token = (string)payload["Token"];
            JToken claims = payload.SelectToken("DisplayClaims.xui[0]");

            string userHash = claims != null ? (string)claims["uhs"] : null;
            string xuid = claims != null ? (string)claims["xid"] : null;

            if (string.IsNullOrEmpty(token) || string.IsNullOrEmpty(userHash))
                throw Fail("Xbox Live yanıtı okunamadı. Lütfen tekrar deneyin.", "XBL-VERI", "xbox-live", "Token/uhs alanlari bos");

            return new XboxToken(token, userHash, xuid);
        }

        /// <summary>
        /// Başarısız yanıtları destek amacıyla dosyaya yazar. Arayüzde teknik ayrıntı gösterilmez;
        /// sorun sürerse bu dosya sebebi tek bakışta ortaya koyar. Yalnızca hatalı yanıtlar
        /// kaydedilir, dolayısıyla dosyaya hiçbir erişim anahtarı düşmez.
        /// </summary>
        private static void LogFailure(Uri requestUri, HttpStatusCode statusCode, string body)
        {
            try
            {
                string path = Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
                    ".projects",
                    "premium-giris.log");

                Directory.CreateDirectory(Path.GetDirectoryName(path));

                string detail = (body ?? string.Empty).Replace("\r", " ").Replace("\n", " ");
                if (detail.Length > 600)
                    detail = detail.Substring(0, 600) + "…";

                File.AppendAllText(
                    path,
                    DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture) +
                    "  " + (int)statusCode + " " + statusCode +
                    "  " + requestUri.GetLeftPart(UriPartial.Path) + Environment.NewLine +
                    "    " + detail + Environment.NewLine,
                    Encoding.UTF8);
            }
            catch (Exception)
            {
                // Günlük yazılamadıysa giriş akışı etkilenmez.
            }
        }

        // --- Hata üretimi ---

        /// <summary>Kullanıcıya kısa kod, destek ekibine panoya kopyalanabilir ayrıntı bırakan hata.</summary>
        private static PremiumAuthException Fail(string message, string code, string step, string detail)
        {
            return new PremiumAuthException(message)
            {
                ErrorCode = code,
                Diagnostics = BuildDiagnostics(code, step, detail),
            };
        }

        private static PremiumAuthException Fail(string message, string code, string step, HttpResponse response)
        {
            return Fail(message, code, step,
                (int)response.StatusCode + " " + response.StatusCode + " · " + response.Body);
        }

        private static string BuildDiagnostics(string code, string step, string detail)
        {
            string version;

            try
            {
                version = Properties.Settings.Default.currentVersion;
            }
            catch (Exception)
            {
                version = "?";
            }

            detail = (detail ?? "-").Replace("\r", " ").Replace("\n", " ");
            if (detail.Length > 400)
                detail = detail.Substring(0, 400) + "…";

            return
                "Projects Launcher v" + version + Environment.NewLine +
                "Hata kodu: " + code + Environment.NewLine +
                "Zaman: " + DateTime.Now.ToString("yyyy-MM-dd HH:mm:ss", CultureInfo.InvariantCulture) + Environment.NewLine +
                "Adim: " + step + Environment.NewLine +
                "Ayrinti: " + detail;
        }

        // Yanıttaki hata kodu, kısa koda ek ayırt edicilik katar (ör. "MSA-invalid_grant").
        private static string ReadErrorTag(string content, string propertyName)
        {
            try
            {
                string value = (string)JObject.Parse(content)[propertyName];
                return string.IsNullOrEmpty(value) ? null : value;
            }
            catch (Exception)
            {
                return null;
            }
        }

        // --- Hata metinleri ---

        /// <summary>
        /// Minecraft oturum ucunun reddi birden çok sebepten olabilir; kullanıcıya kod değil
        /// yapması gereken şey söylenir. Kesin sebep <c>premium-giris.log</c> dosyasındadır.
        /// </summary>
        private static PremiumAuthException CreateMinecraftLoginException(HttpResponse response)
        {
            string code = "MC-" + (int)response.StatusCode;

            // Mojang, üçüncü taraf başlatıcıların uygulama kimliğini onaylamadan Minecraft API'sine
            // sokmuyor. Bu red kullanıcının hesabıyla ilgili değildir; onu yanlış yönlendirmemek
            // için ayrı ele alınır (yoksa "oyunu satın aldın mı?" diye sorardık).
            if (response.Body.IndexOf("app registration", StringComparison.OrdinalIgnoreCase) >= 0)
            {
                return Fail(
                    "Premium giriş şu an kullanılamıyor.\n\n" +
                    "Sorun hesabınızda değil, başlatıcı tarafındaki yetkilendirmede.\n" +
                    "Bu süre boyunca kullanıcı adıyla normal giriş yapabilirsiniz.",
                    "MC-APPREG", "minecraft-oturum", response);
            }

            if (response.StatusCode == HttpStatusCode.Unauthorized || response.StatusCode == HttpStatusCode.Forbidden)
            {
                return Fail(
                    "Minecraft hesabınız doğrulanamadı.\n\n" +
                    "Oyunun bu Microsoft hesabıyla satın alınmış olduğundan emin olun.",
                    code, "minecraft-oturum", response);
            }

            if ((int)response.StatusCode == 429)
            {
                return Fail(
                    "Çok fazla giriş denemesi yapıldı.\nBirkaç dakika bekleyip tekrar deneyin.",
                    code, "minecraft-oturum", response);
            }

            return Fail(
                "Minecraft sunucuları şu an yanıt vermiyor. Lütfen birazdan tekrar deneyin.",
                code, "minecraft-oturum", response);
        }

        private static string DescribeAuthorizeError(string error)
        {
            if (string.Equals(error, "access_denied", StringComparison.OrdinalIgnoreCase))
                return "Giriş izni verilmedi. Premium giriş için Microsoft hesabınızda izni onaylamanız gerekir.";

            return "Microsoft giriş sayfası isteği reddetti. Lütfen tekrar deneyin.";
        }

        private static PremiumAuthException CreateTokenException(string content)
        {
            string error = null;

            try
            {
                error = (string)JObject.Parse(content)["error"];
            }
            catch (Exception)
            {
                // Yanıt JSON değilse genel mesaj kullanılır.
            }

            string code = "MSA-" + (error ?? "HATA");

            if (string.Equals(error, "invalid_grant", StringComparison.OrdinalIgnoreCase) ||
                string.Equals(error, "interaction_required", StringComparison.OrdinalIgnoreCase) ||
                string.Equals(error, "consent_required", StringComparison.OrdinalIgnoreCase))
            {
                PremiumAuthException expired = Fail(
                    "Kayıtlı oturumun süresi dolmuş.\nLütfen Microsoft hesabınızla yeniden giriş yapın.",
                    code, "anahtar", content);

                expired.RequiresSignIn = true;
                return expired;
            }

            if (string.Equals(error, "unauthorized_client", StringComparison.OrdinalIgnoreCase) ||
                string.Equals(error, "invalid_client", StringComparison.OrdinalIgnoreCase))
            {
                return Fail(
                    "Premium giriş şu an kullanılamıyor.\nBaşlatıcıyı güncelleyin ya da destek talebi oluşturun.",
                    code, "anahtar", content);
            }

            return Fail("Microsoft girişi tamamlanamadı. Lütfen tekrar deneyin.", code, "anahtar", content);
        }

        /// <summary>XSTS reddi kullanıcı hesabının durumunu anlatır; her kod ayrı bir çözüm gerektirir.</summary>
        private static string DescribeXstsError(string content)
        {
            long errorCode = 0;

            try
            {
                errorCode = (long?)JObject.Parse(content)["XErr"] ?? 0;
            }
            catch (Exception)
            {
                // Yanıt JSON değilse genel mesaj kullanılır.
            }

            switch (errorCode)
            {
                case 2148916233L:
                    return "Bu Microsoft hesabına bağlı bir Xbox profili yok.\n\n" +
                           "xbox.com adresinden ücretsiz profil oluşturup tekrar deneyin.";
                case 2148916235L:
                    return "Xbox Live, hesabınızın kayıtlı olduğu ülkede kullanılamıyor.";
                case 2148916236L:
                case 2148916237L:
                    return "Hesabınız için yetişkin doğrulaması gerekiyor.\n\n" +
                           "xbox.com üzerinden doğrulamayı tamamlayıp tekrar deneyin.";
                case 2148916238L:
                    return "Bu hesap bir çocuk hesabı olarak işaretli.\n\n" +
                           "Bir aile grubuna eklenmeden Xbox Live'a giriş yapılamaz.";
                default:
                    return "Xbox Live doğrulaması tamamlanamadı. Lütfen tekrar deneyin.";
            }
        }

        // --- PKCE ve sorgu yardımcıları ---

        private static void Report(IProgress<PremiumAuthStage> progress, PremiumAuthStage stage)
        {
            if (progress != null)
                progress.Report(stage);
        }

        private static string CreateRandomToken(int byteCount)
        {
            byte[] buffer = new byte[byteCount];

            using (RandomNumberGenerator random = RandomNumberGenerator.Create())
                random.GetBytes(buffer);

            return Base64UrlEncode(buffer);
        }

        private static string CreateCodeChallenge(string codeVerifier)
        {
            using (SHA256 sha256 = SHA256.Create())
                return Base64UrlEncode(sha256.ComputeHash(Encoding.ASCII.GetBytes(codeVerifier)));
        }

        private static string Base64UrlEncode(byte[] value)
        {
            return Convert.ToBase64String(value)
                .TrimEnd('=')
                .Replace('+', '-')
                .Replace('/', '_');
        }

        private static string BuildQueryString(Dictionary<string, string> parameters)
        {
            StringBuilder query = new StringBuilder();

            foreach (KeyValuePair<string, string> parameter in parameters)
            {
                if (query.Length > 0)
                    query.Append('&');

                query.Append(Uri.EscapeDataString(parameter.Key));
                query.Append('=');
                query.Append(Uri.EscapeDataString(parameter.Value));
            }

            return query.ToString();
        }

        private static void OpenInBrowser(string url)
        {
            try
            {
                Process.Start(url);
            }
            catch (Exception exception)
            {
                throw new PremiumAuthException(
                    "Tarayıcı açılamadı. Varsayılan tarayıcınızı kontrol edip tekrar deneyin.",
                    exception);
            }
        }

        // --- İç tipler ---

        private struct HttpResponse
        {
            public readonly HttpStatusCode StatusCode;
            public readonly bool IsSuccess;
            public readonly string Body;

            public HttpResponse(HttpStatusCode statusCode, bool isSuccess, string body)
            {
                StatusCode = statusCode;
                IsSuccess = isSuccess;
                Body = body ?? string.Empty;
            }
        }

        private sealed class XboxToken
        {
            public readonly string Token;
            public readonly string UserHash;
            public readonly string Xuid;

            public XboxToken(string token, string userHash, string xuid)
            {
                Token = token;
                UserHash = userHash;
                Xuid = xuid;
            }
        }

        /// <summary>
        /// Tarayıcının yönlendirileceği geçici yerel dinleyici. İşletim sisteminin verdiği boş bir
        /// portta yalnızca döngü (loopback) arayüzünü dinler; dışarıdan erişilemez ve giriş biter
        /// bitmez kapanır. HttpListener yerine ham soket kullanılır: HttpListener bazı kurulumlarda
        /// yönetici hakkı olmadan port ayırtamıyor.
        /// </summary>
        private sealed class LoopbackServer : IDisposable
        {
            private readonly List<TcpListener> _listeners = new List<TcpListener>();
            private Task<TcpClient>[] _pendingAccepts;
            private bool _disposed;

            public int Port { get; private set; }

            /// <summary>
            /// Azure'da kayıtlı "http://localhost" adresi, döngü arayüzünde herhangi bir portla
            /// eşleşir. Sondaki eğik çizgi bilerek yok: bazı doğrulamalar yolu birebir karşılaştırır.
            /// </summary>
            public string RedirectUri
            {
                get { return "http://localhost:" + Port.ToString(CultureInfo.InvariantCulture); }
            }

            public LoopbackServer()
            {
                TcpListener ipv4 = new TcpListener(IPAddress.Loopback, 0);
                ipv4.Start();

                Port = ((IPEndPoint)ipv4.LocalEndpoint).Port;
                _listeners.Add(ipv4);

                // Tarayıcı "localhost" adını ::1 olarak çözerse istek IPv4 soketine hiç ulaşmaz;
                // bu yüzden aynı port IPv6 döngü arayüzünde de dinlenir. IPv6 kapalıysa atlanır.
                try
                {
                    if (Socket.OSSupportsIPv6)
                    {
                        TcpListener ipv6 = new TcpListener(IPAddress.IPv6Loopback, Port);
                        ipv6.Start();
                        _listeners.Add(ipv6);
                    }
                }
                catch (SocketException)
                {
                    // IPv6 dinlenemedi; IPv4 tek başına yeterli.
                }
            }

            /// <summary>
            /// Yetki kodunu taşıyan isteği bekler. Tarayıcı yanında /favicon.ico gibi ilgisiz
            /// istekler de gönderebildiği için yalnızca "code" ya da "error" taşıyan istek akışı bitirir.
            /// </summary>
            public async Task<Dictionary<string, string>> WaitForCallbackAsync(CancellationToken cancellationToken)
            {
                using (CancellationTokenSource linked = CancellationTokenSource.CreateLinkedTokenSource(cancellationToken))
                {
                    Task cancellationSignal = Task.Delay(Timeout.Infinite, linked.Token);

                    try
                    {
                        while (true)
                        {
                            TcpClient client = await AcceptAsync(cancellationSignal, cancellationToken).ConfigureAwait(false);

                            using (client)
                            {
                                string requestTarget = await ReadRequestTargetAsync(client).ConfigureAwait(false);
                                Dictionary<string, string> query = ParseQuery(requestTarget);

                                bool isCallback = query.ContainsKey("code") || query.ContainsKey("error");
                                bool isSuccess = query.ContainsKey("code");

                                if (isCallback)
                                    await WriteResultPageAsync(client, isSuccess).ConfigureAwait(false);
                                else
                                    await WriteEmptyResponseAsync(client).ConfigureAwait(false);

                                if (isCallback)
                                    return query;
                            }
                        }
                    }
                    finally
                    {
                        // Bekleyen Task.Delay iptal edilmezse zamanlayıcı kaydı açık kalır.
                        linked.Cancel();
                    }
                }
            }

            private async Task<TcpClient> AcceptAsync(Task cancellationSignal, CancellationToken cancellationToken)
            {
                if (_pendingAccepts == null)
                {
                    _pendingAccepts = new Task<TcpClient>[_listeners.Count];
                    for (int i = 0; i < _listeners.Count; i++)
                        _pendingAccepts[i] = _listeners[i].AcceptTcpClientAsync();
                }

                Task[] candidates = new Task[_pendingAccepts.Length + 1];
                Array.Copy(_pendingAccepts, candidates, _pendingAccepts.Length);
                candidates[_pendingAccepts.Length] = cancellationSignal;

                Task finished = await Task.WhenAny(candidates).ConfigureAwait(false);

                int index = -1;
                for (int i = 0; i < _pendingAccepts.Length; i++)
                {
                    if (ReferenceEquals(_pendingAccepts[i], finished))
                    {
                        index = i;
                        break;
                    }
                }

                if (index < 0)
                    throw new OperationCanceledException(cancellationToken);

                TcpClient client;

                try
                {
                    client = await _pendingAccepts[index].ConfigureAwait(false);
                }
                catch (Exception)
                {
                    // Dinleyici kapandıysa (Dispose) bekleme iptal edilmiş sayılır.
                    throw new OperationCanceledException(cancellationToken);
                }

                // Aynı dinleyici sıradaki istek için yeniden kurulur (favicon vb.).
                _pendingAccepts[index] = _listeners[index].AcceptTcpClientAsync();

                return client;
            }

            private static async Task<string> ReadRequestTargetAsync(TcpClient client)
            {
                // Yalnızca istek satırı gerekir: "GET /?code=... HTTP/1.1"
                NetworkStream stream = client.GetStream();
                byte[] buffer = new byte[2048];
                StringBuilder received = new StringBuilder();

                while (received.Length < 8192)
                {
                    int read = await stream.ReadAsync(buffer, 0, buffer.Length).ConfigureAwait(false);
                    if (read <= 0)
                        break;

                    received.Append(Encoding.ASCII.GetString(buffer, 0, read));

                    if (received.ToString().IndexOf('\n') >= 0)
                        break;
                }

                string requestLine = received.ToString();
                int lineBreak = requestLine.IndexOf('\n');
                if (lineBreak >= 0)
                    requestLine = requestLine.Substring(0, lineBreak);

                string[] parts = requestLine.Trim().Split(' ');
                return parts.Length >= 2 ? parts[1] : string.Empty;
            }

            private static Dictionary<string, string> ParseQuery(string requestTarget)
            {
                Dictionary<string, string> result = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);

                int start = requestTarget.IndexOf('?');
                if (start < 0)
                    return result;

                string[] pairs = requestTarget.Substring(start + 1).Split('&');

                foreach (string pair in pairs)
                {
                    if (pair.Length == 0)
                        continue;

                    int separator = pair.IndexOf('=');
                    string key = separator < 0 ? pair : pair.Substring(0, separator);
                    string value = separator < 0 ? string.Empty : pair.Substring(separator + 1);

                    result[Uri.UnescapeDataString(key)] = Uri.UnescapeDataString(value.Replace('+', ' '));
                }

                return result;
            }

            private static Task WriteResultPageAsync(TcpClient client, bool success)
            {
                string accent = success ? "#22c55e" : "#ef4444";
                string glyph = success ? "&#10003;" : "&#10005;";
                string title = success ? "Giriş başarılı" : "Giriş tamamlanmadı";
                string message = success
                    ? "Bu sekmeyi kapatıp Projects Launcher&#39;a dönebilirsiniz."
                    : "Başlatıcıya dönüp tekrar deneyebilirsiniz.";

                string page =
                    "<!doctype html><html lang=\"tr\"><head><meta charset=\"utf-8\">" +
                    "<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">" +
                    "<title>Projects Launcher</title><style>" +
                    "*{box-sizing:border-box}" +
                    "body{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;" +
                    "background:#14171d;color:#f5f7fa;font-family:'Segoe UI',system-ui,sans-serif}" +
                    ".card{width:min(420px,90vw);padding:40px 36px;text-align:center;border-radius:16px;" +
                    "background:#20242c;border:1px solid #343a46;box-shadow:0 24px 60px rgba(0,0,0,.45)}" +
                    ".mark{width:64px;height:64px;margin:0 auto 22px;border-radius:50%;display:flex;" +
                    "align-items:center;justify-content:center;font-size:30px;color:#14171d;background:" + accent + "}" +
                    "h1{margin:0 0 10px;font-size:21px;font-weight:600}" +
                    "p{margin:0;font-size:14px;line-height:1.6;color:#98a2b3}" +
                    "</style></head><body><div class=\"card\">" +
                    "<div class=\"mark\">" + glyph + "</div>" +
                    "<h1>" + title + "</h1><p>" + message + "</p>" +
                    "</div></body></html>";

                return WriteAsync(client, "200 OK", "text/html; charset=utf-8", page);
            }

            private static Task WriteEmptyResponseAsync(TcpClient client)
            {
                return WriteAsync(client, "404 Not Found", "text/plain; charset=utf-8", string.Empty);
            }

            private static async Task WriteAsync(TcpClient client, string status, string contentType, string body)
            {
                try
                {
                    byte[] payload = Encoding.UTF8.GetBytes(body);
                    byte[] header = Encoding.ASCII.GetBytes(
                        "HTTP/1.1 " + status + "\r\n" +
                        "Content-Type: " + contentType + "\r\n" +
                        "Content-Length: " + payload.Length.ToString(CultureInfo.InvariantCulture) + "\r\n" +
                        "Cache-Control: no-store\r\n" +
                        "Connection: close\r\n\r\n");

                    NetworkStream stream = client.GetStream();
                    await stream.WriteAsync(header, 0, header.Length).ConfigureAwait(false);

                    if (payload.Length > 0)
                        await stream.WriteAsync(payload, 0, payload.Length).ConfigureAwait(false);

                    await stream.FlushAsync().ConfigureAwait(false);
                }
                catch (Exception)
                {
                    // Tarayıcı bağlantıyı erken kapatmış olabilir; giriş akışını etkilemez.
                }
            }

            public void Dispose()
            {
                if (_disposed)
                    return;

                _disposed = true;

                foreach (TcpListener listener in _listeners)
                {
                    try
                    {
                        listener.Stop();
                    }
                    catch (Exception)
                    {
                        // Zaten kapanmış olabilir.
                    }
                }

                _listeners.Clear();
            }
        }
    }
}
