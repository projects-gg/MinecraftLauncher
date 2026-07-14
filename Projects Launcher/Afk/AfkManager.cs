using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;

namespace Projects_Launcher.Afk
{
    /// <summary>
    /// Tüm AFK oturumlarının tekil kayıt defteri. Hesap başına bir <see cref="AfkSession"/> örneği
    /// üretir ve saklar; arayüzdeki farklı pencereler (hesap listesi, konsol vb.) aynı oturuma
    /// bu sınıf üzerinden erişir, böylece aynı hesap için birden fazla süreç başlatılmaz.
    /// </summary>
    public class AfkManager : IDisposable
    {
        // Hesap kaldırılırken oturumun düzgünce kapanması için beklenecek en fazla süre.
        private const int RemoveGracePeriodMs = 4000;

        private static readonly object instanceGate = new object();
        private static AfkManager instance;

        /// <summary>Tembel (lazy) tekil örnek; ilk erişimde oluşturulur.</summary>
        public static AfkManager Instance
        {
            get
            {
                lock (instanceGate)
                {
                    if (instance == null)
                        instance = new AfkManager();
                    return instance;
                }
            }
        }

        private readonly object gate = new object();
        private readonly Dictionary<string, AfkSession> sessions = new Dictionary<string, AfkSession>();
        private bool disposed;

        private AfkManager()
        {
        }

        /// <summary>Hesabın oturumunu döndürür; daha önce oluşturulmadıysa yenisini açıp önbelleğe alır.</summary>
        public AfkSession GetSession(AfkAccount account)
        {
            if (account == null) throw new ArgumentNullException("account");

            lock (gate)
            {
                AfkSession session;
                if (sessions.TryGetValue(account.Id, out session))
                {
                    // Ayarlar kaydedilince liste hesabı yeni bir nesneyle değiştirir; oturum eski
                    // nesnede kalırsa yeniden bağlanışta bile eski ayarlar yazılırdı.
                    if (!ReferenceEquals(session.Account, account))
                        session.UpdateAccount(account);

                    return session;
                }

                session = new AfkSession(account);
                sessions[account.Id] = session;
                return session;
            }
        }

        /// <summary>Hesap kimliğine göre halihazırda açılmış bir oturum varsa döndürür.</summary>
        public bool TryGetSession(string accountId, out AfkSession session)
        {
            if (accountId == null)
            {
                session = null;
                return false;
            }

            lock (gate)
                return sessions.TryGetValue(accountId, out session);
        }

        /// <summary>Oturumu durdurur, kaynaklarını serbest bırakır ve kayıt defterinden çıkarır.</summary>
        public void Remove(AfkAccount account)
        {
            if (account == null) throw new ArgumentNullException("account");

            AfkSession session;
            lock (gate)
            {
                if (!sessions.TryGetValue(account.Id, out session))
                    return;

                // Önce sözlükten çıkarılır: teardown sürerken başka bir çağrı aynı oturumu
                // tekrar almaya çalışırsa yarı kapanmış bir örnekle karşılaşmasın.
                sessions.Remove(account.Id);
            }

            try
            {
                // Hesap tamamen silinmeden önce sürecin gerçekten kapandığından emin olunur;
                // aksi halde MCC süreci sahipsiz kalıp arka planda çalışmaya devam edebilir.
                session.StopBlocking(RemoveGracePeriodMs);
            }
            finally
            {
                session.Dispose();
            }
        }

        /// <summary>
        /// Başlatıcı kapanırken çağrılır. Önce tüm oturumlara <see cref="AfkSession.Stop"/> gönderilir
        /// (MCC'ye /quit yollar, bloklamaz); ardından her biri için kalan süre kadar
        /// <see cref="AfkSession.StopBlocking"/> çağrılıp sürecin gerçekten kapandığından emin olunur.
        /// Böylece toplam bekleme oturum sayısıyla çarpılmadan timeoutMs'e yakın kalır.
        /// </summary>
        public void StopAll(int timeoutMs)
        {
            AfkSession[] snapshot;
            lock (gate)
                snapshot = sessions.Values.ToArray();

            foreach (AfkSession session in snapshot)
                session.Stop();

            Stopwatch watch = Stopwatch.StartNew();
            foreach (AfkSession session in snapshot)
            {
                int remaining = timeoutMs - (int)watch.ElapsedMilliseconds;
                if (remaining < 0)
                    remaining = 0;

                session.StopBlocking(remaining);
            }
        }

        /// <summary>Açık oturumların anlık bir kopyası (numaralandırma sırasında kilit tutulmaz).</summary>
        public IEnumerable<AfkSession> Sessions
        {
            get
            {
                lock (gate)
                    return sessions.Values.ToArray();
            }
        }

        public void Dispose()
        {
            if (disposed)
                return;
            disposed = true;

            AfkSession[] snapshot;
            lock (gate)
            {
                snapshot = sessions.Values.ToArray();
                sessions.Clear();
            }

            foreach (AfkSession session in snapshot)
                session.Dispose();
        }
    }
}
