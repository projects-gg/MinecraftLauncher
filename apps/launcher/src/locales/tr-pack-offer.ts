// PACK-OFFER: the official pack suggested where the player picks a Minecraft version by hand (a new
// instance, an empty one) and the family's newer versions told once on the official instance.
// Short, "sen"; buttons in Title Case (owner, 7 Nov), notes as sentences. No ids or file names.
export const trPackOffer = {
  /** A version picked by hand has the official pack: the first sentence. */
  title: "Bu sürüm için resmî paket var.",
  /** No official instance: what installing brings. */
  installBody: "{{name}} paketi bu sürüm için seçilmiş modlarla gelir.",
  install: "Resmî Paketi Kur",
  installed: "{{name}} kuruldu.",
  /** The official instance is on another version; the button says what the version dialog says. */
  switchBody: "{{name}} kurulumunu Minecraft {{minecraft}} sürümüne geçirebilirsin.",
  switch: "{{minecraft}} Sürümüne Geç",
  /** The official instance is already on this version. */
  openBody: "{{name}} kurulumun zaten bu sürümde.",
  open: "Kurulumu Aç",
  /** The official instance: a newer version of its family it was not told about. */
  newer: "Bu paket Minecraft {{minecraft}} için de var.",
  /** Ready-made sets for the version picked by hand ("Sıfırdan"). */
  sets: "Bu sürüm için hazır setler var.",
  setsAction: "Setlere Bak",
  dismiss: "Gizle",
};
