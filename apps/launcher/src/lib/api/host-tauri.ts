// Tauri implementation of `hostApi` (HOST1, "Dünyanı arkadaşlarına aç"): HOST1-L1's commands
// (`src-tauri/src/commands/host.rs`) and events (`host-changed`, `host-notice`).
//
// The backend builds its views field for field like host-types.ts and leaves absent optional
// fields out (never `null`), so each call is the command's data as it is; the bindings only spell
// optional fields as `T | null`, hence the one assertion per shape below. The feature stays hidden
// (`enabled: false`) until the signed config's `features.worlds` and the friends service's
// `worlds` are both on. A failed command rejects with a `HostCommandError` (`message` in the
// player's language; `code` and `details` for "Ayrıntıları kopyala").
import { listen } from "@tauri-apps/api/event";
import {
  type CmdError,
  commands,
  type HostNotice as BindingNotice,
  type HostView as BindingView,
  type HostPlace as BindingPlace,
} from "@/bindings";
import type { HostApi, HostNotice, HostPlace, HostView } from "./host-types";

/** A failed command: friendly `message`, plus `code`/`details` for "copy details". */
export class HostCommandError extends Error {
  readonly code: string;
  readonly details: string;
  constructor(error: CmdError) {
    super(error.message);
    this.code = error.code;
    this.details = error.details;
  }
}

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function ok<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new HostCommandError(result.error);
  return result.data;
}

// The same shapes; absent optional fields never arrive as `null` (see the top of the file).
const asView = (view: BindingView) => view as unknown as HostView;
const asNotice = (notice: BindingNotice) => notice as unknown as HostNotice;
const asPlace = (place: BindingPlace) => place as unknown as HostPlace;

export const tauriHost: HostApi = {
  async state() {
    return asView(await ok(commands.hostState()));
  },
  onChange(listener) {
    const unlisten = listen<BindingView>("host-changed", (event) => listener(asView(event.payload)));
    return () => {
      void unlisten.then((stop) => stop());
    };
  },
  onNotice(listener) {
    const unlisten = listen<BindingNotice>("host-notice", (event) =>
      listener(asNotice(event.payload)),
    );
    return () => {
      void unlisten.then((stop) => stop());
    };
  },
  async place(instanceId) {
    return asPlace(await ok(commands.hostPlace(instanceId)));
  },
  open: (request) => ok(commands.hostOpen(request)),
  cancelOpen: () => ok(commands.hostCancelOpen()),
  invite: (friends) => ok(commands.hostInvite(friends)),
  remove: (friend) => ok(commands.hostRemove(friend)),
  stop: () => ok(commands.hostStop()),
  dismissFailure: () => ok(commands.hostDismissFailure()),
  meetOnProjects: (friends) => ok(commands.hostMeetOnProjects(friends)),
  join: (session, options) => ok(commands.hostJoin(session, options?.copy?.name ?? null)),
  answer: (session, answer) => ok(commands.hostAnswer(session, answer)),
  cancelJoin: (session) => ok(commands.hostCancelJoin(session)),
  dismissInvite: (session) => ok(commands.hostDismissInvite(session)),
  connectInGame: (session, label) => ok(commands.hostConnectInGame(session, label)),
};
