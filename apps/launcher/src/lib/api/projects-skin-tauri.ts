// Tauri implementation of `projectsSkinApi` (SKIN1): the commands of
// `src-tauri/src/commands/projects_skin.rs` over `pl_social::ProjectsSkins`. The views match
// projects-skin-types.ts field for field; only the optional details the backend sends as `null`
// (a refusal's or a result's `name`, a refusal's `retryAfter`) become `undefined` here.
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import {
  type ProjectsSkinChange as ChangeBinding,
  type CmdError,
  commands,
  type ProjectsSkinJob as JobBinding,
  type ProjectsSkinOutcome as OutcomeBinding,
  type ProjectsSkinRefusal as RefusalBinding,
  type ProjectsSkinView as ViewBinding,
} from "@/bindings";
import type {
  ProjectsSkinApi,
  ProjectsSkinChange,
  ProjectsSkinJob,
  ProjectsSkinOutcome,
  ProjectsSkinRefusal,
  ProjectsSkinView,
} from "./projects-skin-types";

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

type Res<T> = { status: "ok"; data: T } | { status: "error"; error: CmdError };

async function ok<T>(promise: Promise<Res<T>>): Promise<T> {
  const result = await promise;
  if (result.status === "error") throw new ProjectsSkinCommandError(result.error);
  return result.data;
}

function job(v: JobBinding): ProjectsSkinJob {
  return {
    ...v,
    result: v.result
      ? {
          code: v.result.code,
          ...(v.result.name ? { name: v.result.name } : {}),
          ...(v.result.until !== undefined ? { until: v.result.until } : {}),
        }
      : null,
  };
}

function refusal(v: RefusalBinding): ProjectsSkinRefusal {
  return {
    code: v.code,
    ...(v.until !== undefined ? { until: v.until } : {}),
    ...(v.name ? { name: v.name } : {}),
    ...(typeof v.retryAfter === "number" ? { retryAfter: v.retryAfter } : {}),
  };
}

function outcome(v: OutcomeBinding): ProjectsSkinOutcome {
  return v.status === "sent"
    ? { status: "sent", job: job(v.job) }
    : { status: "refused", refusal: refusal(v.refusal) };
}

function view(v: ViewBinding): ProjectsSkinView {
  return {
    ...v,
    job: v.job ? job(v.job) : null,
    lastJob: v.lastJob ? job(v.lastJob) : null,
    linkJob: v.linkJob ? job(v.linkJob) : null,
  };
}

function change(v: ChangeBinding): ProjectsSkinChange {
  return v.kind === "job" ? { ...v, job: job(v.job) } : v;
}

export const tauriProjectsSkin: ProjectsSkinApi = {
  overview: () => ok(commands.projectsSkinOverview()),
  state: async (accountId) => view(await ok(commands.projectsSkinState(accountId))),
  defaultLook: (accountId) => ok(commands.projectsSkinDefault(accountId)),
  async pickFile() {
    const chosen = await open({
      multiple: false,
      directory: false,
      filters: [{ name: "Skin (PNG)", extensions: ["png"] }],
    });
    return typeof chosen === "string" ? chosen : null;
  },
  readFile: (path) => ok(commands.projectsSkinPickFile(path)),
  lookup: (accountId, name) => ok(commands.projectsSkinLookup(accountId, name)),
  save: async (accountId, save) => outcome(await ok(commands.projectsSkinSave(accountId, save))),
  cancel: (jobId) => ok(commands.projectsSkinCancel(jobId)),
  link: async (accountId, pin) => outcome(await ok(commands.projectsLink(accountId, pin))),
  unlink: async () => outcome(await ok(commands.projectsUnlink())),
  linkCheck: () => ok(commands.projectsLinkCheck()).then(() => undefined),
  onChange(listener) {
    const unlisten = listen<ChangeBinding>("projects-skin-changed", (event) =>
      listener(change(event.payload)),
    );
    return () => {
      void unlisten.then((fn) => fn());
    };
  },
};
