// QOL-L: the launcher's quality-of-life pass (docs/tasks/qol-l.md). Short, sentence case, no codes.
export const trQol = {
  picker: {
    /**
     * The command bar's installation menu while a launch prepares its files: the other rows are
     * off, and this says how to free them (the bar's "İptal et").
     */
    lockedPreparing: "Başka kurulum seçmek için önce hazırlığı iptal et.",
    /** The same menu while the game opens or runs. */
    lockedPlaying: "Oyun kapanınca başka kurulum seçebilirsin.",
  },
  mods: {
    /** Modlar → Önerilenler while a game of the instance prepares or runs: every switch is off. */
    locked: "Oyun açıkken modlar değiştirilemez.",
    /** Modlar → Setler, the same. */
    setsLocked: "Oyun açıkken setler değiştirilemez.",
  },
};
