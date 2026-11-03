// SOC-NEXT's one switch: the signed config's `features.socNext` (with `features.friends`), read
// from the home content and kept here, so rows, menus and the background rules (who is heard, a
// nickname shown) ask one place. Off: nothing of SOC-NEXT shows and its rules leave everything
// as before.
import { create } from "zustand";

export const useSocNextFlag = create<{ on: boolean }>()(() => ({ on: false }));

/** SOC-NEXT's screens show (React). */
export function useSocNext(): boolean {
  return useSocNextFlag((s) => s.on);
}

/** SOC-NEXT's screens show (outside React: notices, the snapshot's names). */
export function socNextOn(): boolean {
  return useSocNextFlag.getState().on;
}
