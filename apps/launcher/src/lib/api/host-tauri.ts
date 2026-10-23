// Tauri implementation of `hostApi` (HOST1, "Dünyanı arkadaşlarına aç").
//
// ─── HOST1-L1 PLUGS IN HERE ─────────────────────────────────────────────────────────────────────
// The Rust commands do not exist yet (docs/tasks/host1-l2.md lists the names and shapes the
// interface assumes). Until they do, the view says the feature is off, so no entry point shows
// (the worlds' "Arkadaşlarınla oyna", the friends panel's strip, world invites, the person menu's
// world rows), and every other call refuses. Once `bindings.ts` has them, replace the stubs below:
//
//   state:          ok(commands.hostState())
//   onChange:       listen<HostView>("host-changed", (event) => listener(event.payload))
//   onNotice:       listen<HostNotice>("host-notice", (event) => listener(event.payload))
//   place:          ok(commands.hostPlace(instanceId))
//   open:           ok(commands.hostOpen(request))
//   cancelOpen:     ok(commands.hostCancelOpen())
//   invite:         ok(commands.hostInvite(friends))
//   remove:         ok(commands.hostRemove(friend))
//   stop:           ok(commands.hostStop())
//   dismissFailure: ok(commands.hostDismissFailure())
//   meetOnProjects: ok(commands.hostMeetOnProjects(friends))
//   join:           ok(commands.hostJoin(session, options?.copy?.name ?? null))
//   answer:         ok(commands.hostAnswer(session, answer))
//   cancelJoin:     ok(commands.hostCancelJoin(session))
//   dismissInvite:  ok(commands.hostDismissInvite(session))
//   connectInGame:  ok(commands.hostConnectInGame(session, label))
//
// The views are built to match host-types.ts field for field, so each call is the command's data
// as it is (map here only where a binding differs). A failed command rejects with a `CmdError`
// (`message` in the player's language; `code` and `details` for "Ayrıntıları kopyala").
// ─────────────────────────────────────────────────────────────────────────────────────────────────
import type { CmdError } from "@/bindings";
import { HIDDEN_HOST_VIEW, type HostApi } from "./host-types";

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

/** Until HOST1-L1: the commands are missing, so nothing of the feature runs. */
const missing = (): never => {
  throw new HostCommandError({
    code: "DUNYA-YOK",
    message: "",
    details: "host commands are not in this build",
  });
};

export const tauriHost: HostApi = {
  async state() {
    return HIDDEN_HOST_VIEW;
  },
  onChange: () => () => {},
  onNotice: () => () => {},
  async place() {
    return { shown: false };
  },
  open: async () => missing(),
  cancelOpen: async () => missing(),
  invite: async () => missing(),
  remove: async () => missing(),
  stop: async () => missing(),
  dismissFailure: async () => missing(),
  meetOnProjects: async () => missing(),
  join: async () => missing(),
  answer: async () => missing(),
  cancelJoin: async () => missing(),
  dismissInvite: async () => missing(),
  async connectInGame() {
    return false;
  },
};
