/**
 * A text link, or a line's inline action ("Golge_Avci kullan", "Klasörü aç", "Tekrar dene"): bone
 * words, always underlined, the line full under the pointer, a 97% press; a disabled one fades and
 * drops its line. Never gold: the screen's one gold is its main action (docs/design.md §2; BETA2-FIX
 * moved the launcher's gold text links here). Weight and size come from the sentence it sits in.
 */
export const TEXT_LINK =
  "cursor-pointer rounded-sm text-bone-100 underline decoration-white/35 underline-offset-[3px] outline-none transition-[color,text-decoration-color,opacity,scale] duration-150 hover:decoration-bone-100 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.97] disabled:cursor-default disabled:no-underline disabled:opacity-45";
