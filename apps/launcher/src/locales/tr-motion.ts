// BETA-FB: Settings → Görünüm "Animasyonları azalt" (the overlay's F7 settings use the same words),
// fades only. Performance mode and the system's own setting turn it on too, and then the embers
// stay off. Root key `motion`.
export const trMotion = {
  title: "Animasyonları azalt",
  /** What it does, apart from Performance mode right above it (which also drops the effects). */
  help: "Pencereler ve menüler kaymadan açılır.",
  /** Under the switch while Performance mode keeps it on. */
  perfOn: "Performans modu açıkken açık kalır.",
  /** Under the switch while the system keeps it on ("Windows ayarlarında açık."). */
  systemOn: "{{system}} ayarlarında açık.",
  /** Under the embers' switch while it is held off. */
  embersPerf: "Performans modu açıkken kapalı kalır.",
  embersReduced: "Animasyonlar azaltıldığı için kapalı.",
};
