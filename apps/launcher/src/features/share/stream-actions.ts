// What the stage's controls do (SQOL-U1): full screen (a tile's own brings that share forward and
// gives the layout back on leaving), bringing a share forward, the "Yayın" window and its "Her Zaman
// Üstte". The launcher does the window's part; the page shows it at once.
import { shareApi } from "@/share/api";
import { useShareUi } from "./store";

/** Full screen; a tile's own (`forward`) brings its share forward while it lasts. */
export function enterFullscreen(forward?: string) {
  const ui = useShareUi.getState();
  if (ui.full) return;
  useShareUi.setState({
    full: true,
    ...(forward && forward !== ui.forward ? { forward, forwardBefore: ui.forward } : {}),
  });
  void shareApi.fullscreen(true).catch(() => {});
}

/**
 * Full screen gives way (Esc, the tiles' "Tam Ekrandan Çık", the room panel opening over the stage:
 * SQOL-U3's `openRoomPanel`). Safe to call any time: nothing happens outside full screen; a share a
 * tile's "Tam Ekran" brought forward goes back.
 */
export function leaveFullscreen() {
  const ui = useShareUi.getState();
  if (!ui.full) return;
  useShareUi.setState({
    full: false,
    ...(ui.forwardBefore !== undefined ? { forward: ui.forwardBefore } : {}),
    forwardBefore: undefined,
  });
  void shareApi.fullscreen(false).catch(() => {});
}

export function toggleFullscreen(forward?: string) {
  if (useShareUi.getState().full) leaveFullscreen();
  else enterFullscreen(forward);
}

/** "Bu Yayını Öne Al" (its `shareKey`), or "Yan Yana Göster" (null). */
export function bringForward(key: string | null) {
  useShareUi.setState({ forward: key, forwardBefore: undefined });
}

/** "Ayrı Pencerede Aç", or "Launcher'a Geri Al". */
export function popOut(open: boolean) {
  void shareApi.popout(open).catch(() => {});
}

/** "Her Zaman Üstte" (the "Yayın" window). */
export function keepOnTop(on: boolean) {
  void shareApi.onTop(on).catch(() => {});
}
