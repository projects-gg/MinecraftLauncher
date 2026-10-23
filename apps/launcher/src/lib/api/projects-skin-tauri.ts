// Tauri implementation of `projectsSkinApi` (SKIN1).
//
// ─── SKIN1-R PLUGS IN HERE ──────────────────────────────────────────────────────────────────────
// The Rust commands do not exist yet (docs/tasks/skin1-l-r.md lists the names and shapes the
// interface assumes). Until they do, the app reports the feature as not shown, so no entry point
// (the account card's T-shirt, the link row, the palette) ever appears, and every other call
// refuses. Once `bindings.ts` has them, replace the stubs below with the generated calls:
//
//   overview:   ok(commands.projectsSkinOverview())
//   state:      ok(commands.projectsSkinState(accountId))
//   defaultLook: ok(commands.projectsSkinDefault(accountId))
//   readFile:   ok(commands.projectsSkinPickFile(path))
//   lookup:     ok(commands.projectsSkinLookup(accountId, name))
//   save:       ok(commands.projectsSkinSave(accountId, save))
//   cancel:     ok(commands.projectsSkinCancel(jobId))
//   link:       ok(commands.projectsLink(accountId, pin))
//   unlink:     ok(commands.projectsUnlink())
//   linkCheck:  ok(commands.projectsLinkCheck())
//   onChange:   listen<ProjectsSkinChange>("projects-skin-changed", (event) => listener(event.payload))
//
// The views are built to match the types in projects-skin-types.ts field for field, so each call
// is the command's data as it is (map here only where a binding differs).
// ─────────────────────────────────────────────────────────────────────────────────────────────────
import { open } from "@tauri-apps/plugin-dialog";
import type { CmdError } from "@/bindings";
import type { ProjectsSkinApi } from "./projects-skin-types";

/** A failed command: friendly `message`, plus `code`/`details` for "copy details". */
export class ProjectsSkinCommandError extends Error {
  readonly code: string;
  readonly details: string;
  constructor(error: CmdError) {
    super(error.message);
    this.code = error.code;
    this.details = error.details;
  }
}

/** Until SKIN1-R: the commands are missing, so nothing of the feature runs. */
const missing = (): never => {
  throw new ProjectsSkinCommandError({
    code: "PSKIN-YOK",
    message: "",
    details: "projects_skin commands are not in this build",
  });
};

export const tauriProjectsSkin: ProjectsSkinApi = {
  async overview() {
    return { shown: false, service: null, link: null };
  },
  state: async () => missing(),
  defaultLook: async () => missing(),
  async pickFile() {
    const chosen = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Skin (PNG)", extensions: ["png"] }],
    });
    return typeof chosen === "string" ? chosen : null;
  },
  readFile: async () => missing(),
  lookup: async () => missing(),
  save: async () => missing(),
  cancel: async () => missing(),
  link: async () => missing(),
  unlink: async () => missing(),
  linkCheck: async () => missing(),
  onChange: () => () => {},
};
