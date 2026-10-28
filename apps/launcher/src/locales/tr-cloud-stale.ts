// CLOUD-STALE: 7/24 Bulut while its machine cannot be reached. The page says what it knew last, dim
// and in the past with its time, never as live, and the actions that need the machine wait. Short,
// sentence case, "sen". Root key `cloudStale`.
export const trCloudStale = {
  /** One line over the 7/24 Bulut list (QA-FIX-L's words for the machine away). */
  line: "7/24 Bulut'a şu an ulaşılamıyor.",
  /** A card's and the console's state line: the clock time with its ending ("14:05'te"). */
  last: {
    running: "En son {{time}}'{{ending}} oyundaydı",
    connecting: "En son {{time}}'{{ending}} bağlanıyordu",
    queued: "En son {{time}}'{{ending}} sıradaydı",
    stopped: "En son {{time}}'{{ending}} kapalıydı",
  },
  /** Nothing known of it yet. */
  unknown: "Durumu bilinmiyor",
  /** Why Başlat, Durdur and the move between places wait (their tooltip). */
  waits: "7/24 Bulut dönünce kullanabilirsin.",
  console: {
    /** The command line while its machine is away. */
    offline: "7/24 Bulut dönünce yazabilirsin.",
    /** An empty console while its machine is away. */
    noLines: "Satırlar 7/24 Bulut dönünce görünür.",
  },
};
