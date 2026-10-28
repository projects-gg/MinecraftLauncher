// CLOUD-STALE: 7/24 Bulut while its machine cannot be reached. The page says what it knew last, dim
// and with its time, never as live. Short, sentence case, "sen". Root key `cloudStale`.
export const trCloudStale = {
  /** One line over the 7/24 Bulut list (QA-FIX-L's words for the machine away). */
  line: "7/24 Bulut'a şu an ulaşılamıyor.",
  /** A card's and the console's state line: the clock time with its ending ("14:05'te"). */
  last: {
    running: "Son bilinen: {{time}}'{{ending}} oyunda",
    connecting: "Son bilinen: {{time}}'{{ending}} bağlanıyor",
    queued: "Son bilinen: {{time}}'{{ending}} sırada",
    stopped: "Son bilinen: {{time}}'{{ending}} durdurulmuş",
  },
  console: {
    /** The command line while the account is meant to run and its machine is away. */
    offline: "Bağlantı dönünce yazabilirsin.",
    /** An empty console while its machine is away. */
    noLines: "Satırlar bağlantı dönünce görünür.",
  },
};
