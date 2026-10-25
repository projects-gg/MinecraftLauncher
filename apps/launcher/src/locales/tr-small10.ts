// SMALL10: the launcher's small findings (docs/tasks/small10.md). Short, sentence case, no codes.
export const trSmall10 = {
  menu: {
    /**
     * A friend's invite in the friends panel (a world or a Discord invite): the ×'s tooltip and the
     * same row in its menu. It turns the invite down for good.
     */
    dismissInvite: "Daveti kapat",
    /** Home's lines: their ×, and the same row in their menu. The line goes from Home only. */
    hideInvite: "Daveti gizle",
    hideCall: "Çağrıyı gizle",
    hidePlan: "Planı gizle",
  },
  crew: {
    /** After the crew header's "4/6": crewmates online out of all of them. */
    online: "çevrimiçi",
  },
  afk: {
    /** A stopped account that connects by itself when the launcher opens (its AFK settings). */
    stoppedAutoStart: "Durduruldu, launcher açılınca bağlanır",
    /**
     * A desktop account whose twin runs in 7/24 Bulut: the twin's state, its first letter small
     * ("Bulutta oyunda, 3 sa 12 dk", "Bulutta yeniden bağlanacak, 13 sn").
     */
    inCloud: "Bulutta {{state}}",
  },
};
