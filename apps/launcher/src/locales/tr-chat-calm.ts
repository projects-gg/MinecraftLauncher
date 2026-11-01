// Calm chat (CHAT-CALM): a conversation's messages told in one notice ("Kaan: 5 yeni mesaj"), a
// crowded crew that starts muted, and crew chat's "Yavaş mod". Short, sentence case; a crew's or a
// player's name never takes a suffix. Spans in a sentence take whole words ("30 saniye"), labels
// the short units of `units` ("30 sn").
export const trChatCalm = {
  newMessages_one: "{{count}} yeni mesaj",
  newMessages_other: "{{count}} yeni mesaj",
  crowded: "{{crew}} kalabalık, bu yüzden senin için sessize alındı.",
  // The crew page's section of the chat's rules (the leader and moderators).
  chatTitle: "Sohbet",
  slow: {
    title: "Yavaş mod",
    off: "Kapalı",
    spanSeconds_one: "{{count}} saniye",
    spanSeconds_other: "{{count}} saniye",
    placeholder: "{{text}} (yavaş mod: {{time}})",
    // Writing goes on meanwhile; only sending waits.
    wait_one: "Göndermek için {{count}} saniye bekle",
    wait_other: "Göndermek için {{count}} saniye bekle",
    failure: "Yavaş mod açık. Biraz bekle.",
    onLine: "Üyeler iki mesaj arasında {{time}} bekler.",
    offLine: "Üyeler beklemeden yazar.",
    set: "Yavaş mod: {{time}}",
    setOff: "Yavaş mod kapandı",
  },
};
