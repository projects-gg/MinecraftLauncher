// An AFK account between two connections whose next attempt has no time yet: the dropped client
// is still closing, or the attempt is just starting. Short, sentence case.
export const trAfkReconnect = {
  /** The state line until the countdown ("Yeniden bağlanıyor, 5 sn") is known; never a guess. */
  pending: "Yeniden bağlanıyor",
};
