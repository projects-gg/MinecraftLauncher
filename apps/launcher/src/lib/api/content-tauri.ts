// Tauri implementation of `api.content`: maps the generated bindings onto the UI shapes.
import { Channel, convertFileSrc } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  type AddonInstallReport,
  type CmdError,
  type ContentEntryView,
  commands,
  type DatapackView,
  type InstanceView,
  type JobProgressView,
  type ModpackImportView,
  type ModrinthProject,
  type ModrinthSearchHit,
  type ModrinthVersion,
  type PackPreview as PackPreviewView,
} from "@/bindings";
import type {
  ContentApi,
  ContentHit,
  ContentInstallResult,
  ContentLink,
  ContentProject,
  ContentProjectType,
  ContentVersion,
  Datapack,
  InstalledContent,
  JobProgress,
  ModpackImportResult,
  PackPreview,
  PickKind,
  VersionType,
} from "./content-types";
import type { InstanceIcon, InstanceSummary } from "./types";

/** A failed content command: friendly `message`, plus `code`/`details` for "copy details". */
export class ContentCommandError extends Error {
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
  if (result.status === "error") throw new ContentCommandError(result.error);
  return result.data;
}

/** Loader names Modrinth mixes into categories; the UI shows them separately. */
const LOADER_TAGS = new Set([
  "fabric",
  "forge",
  "neoforge",
  "quilt",
  "liteloader",
  "rift",
  "modloader",
  "babric",
  "legacy-fabric",
  "ornithe",
  "nilloader",
  "bta-babric",
  "java-agent",
  "minecraft",
  "iris",
  "optifine",
  "canvas",
  "vanilla",
  "datapack",
  "bukkit",
  "paper",
  "spigot",
  "purpur",
  "folia",
  "sponge",
  "bungeecord",
  "velocity",
  "waterfall",
]);

const projectTypes: ContentProjectType[] = ["mod", "resourcepack", "shader", "modpack", "datapack"];

function projectType(type: string): ContentProjectType | "other" {
  return (projectTypes as string[]).includes(type) ? (type as ContentProjectType) : "other";
}

function versionType(type: string): VersionType {
  return type === "beta" || type === "alpha" ? type : "release";
}

function hit(h: ModrinthSearchHit): ContentHit {
  return {
    projectId: h.projectId,
    slug: h.slug ?? undefined,
    title: h.title,
    author: h.author,
    description: h.description,
    iconUrl: h.iconUrl || undefined,
    downloads: h.downloads,
    follows: h.follows,
    dateModified: h.dateModified,
    categories: (h.displayCategories ?? h.categories).filter((c) => !LOADER_TAGS.has(c)),
  };
}

function project(p: ModrinthProject): ContentProject {
  const links: ContentLink[] = [];
  if (p.sourceUrl) links.push({ kind: "source", url: p.sourceUrl });
  if (p.issuesUrl) links.push({ kind: "issues", url: p.issuesUrl });
  if (p.wikiUrl) links.push({ kind: "wiki", url: p.wikiUrl });
  if (p.discordUrl) links.push({ kind: "discord", url: p.discordUrl });
  const donation = p.donationUrls?.[0];
  if (donation) links.push({ kind: "donate", url: donation.url });
  const gallery = [...(p.gallery ?? [])].sort(
    (a, b) => Number(b.featured) - Number(a.featured) || (a.ordering ?? 0) - (b.ordering ?? 0),
  );
  return {
    id: p.id,
    slug: p.slug ?? undefined,
    projectType: projectType(p.projectType),
    title: p.title,
    description: p.description,
    body: p.body,
    iconUrl: p.iconUrl || undefined,
    downloads: p.downloads,
    followers: p.followers,
    published: p.published,
    updated: p.updated,
    categories: [...p.categories, ...(p.additionalCategories ?? [])].filter(
      (c) => !LOADER_TAGS.has(c),
    ),
    gameVersions: p.gameVersions,
    loaders: p.loaders,
    license: p.license?.name || p.license?.id || undefined,
    links,
    gallery: gallery.map((g) => ({
      url: g.url,
      fullUrl: g.rawUrl ?? undefined,
      title: g.title ?? undefined,
      description: g.description ?? undefined,
      featured: g.featured,
    })),
  };
}

function version(v: ModrinthVersion): ContentVersion {
  const file = v.files.find((f) => f.primary) ?? v.files[0];
  return {
    id: v.id,
    projectId: v.projectId,
    name: v.name,
    versionNumber: v.versionNumber,
    versionType: versionType(v.versionType),
    datePublished: v.datePublished,
    downloads: v.downloads,
    gameVersions: v.gameVersions,
    loaders: v.loaders,
    fileName: file?.filename,
    fileSize: file?.size,
    requiredDependencies: v.dependencies.filter((d) => d.dependencyType === "required").length,
  };
}

function installed(e: ContentEntryView): InstalledContent {
  return {
    fileName: e.fileName,
    enabled: e.enabled,
    sizeBytes: e.size,
    projectId: e.source?.projectId,
    versionId: e.source?.versionId,
    versionNumber: e.source?.versionNumber ?? undefined,
    title: e.source?.title ?? undefined,
    iconUrl: e.iconPath ? convertFileSrc(e.iconPath) : (e.source?.iconUrl ?? undefined),
    fromPack: e.fromPack,
    update: e.update
      ? {
          versionId: e.update.versionId,
          versionNumber: e.update.versionNumber,
          versionType: versionType(e.update.versionType),
          datePublished: e.update.datePublished,
        }
      : undefined,
  };
}

function report(r: AddonInstallReport): ContentInstallResult {
  const item = (i: AddonInstallReport["installed"][number]) => ({
    projectId: i.projectId,
    title: i.title ?? undefined,
    versionNumber: i.versionNumber,
    fileName: i.fileName,
    dependency: i.dependency,
  });
  return {
    installed: r.installed.map(item),
    alreadyInstalled: r.alreadyInstalled.map(item),
    replaced: r.replaced,
    unresolved: r.unresolved.map((u) => ({
      title: u.title ?? undefined,
      fileName: u.fileName ?? undefined,
    })),
    conflicts: r.conflicts.map((c) => ({ title: c.title ?? undefined, projectId: c.projectId })),
  };
}

function progress(p: JobProgressView): JobProgress {
  return {
    stage: p.stage,
    percent: p.percent ?? undefined,
    doneFiles: p.doneFiles,
    totalFiles: p.totalFiles,
    bytesPerSec: p.bytesPerSec,
  };
}

function channel(onProgress: (p: JobProgress) => void): Channel<JobProgressView> {
  const ch = new Channel<JobProgressView>();
  ch.onmessage = (p) => onProgress(progress(p));
  return ch;
}

function instanceSummary(v: InstanceView): InstanceSummary {
  return {
    id: v.id,
    name: v.name,
    icon: v.icon as InstanceIcon,
    mcVersion: v.mcVersion,
    loader: v.loader,
    loaderVersion: v.loaderVersion ?? undefined,
    managed: v.managed,
    lastPlayedAt: v.lastPlayedAt ?? undefined,
    playTimeSecs: v.playTimeSecs,
    state: v.state as InstanceSummary["state"],
    autoJoin: v.autoJoin ?? undefined,
  };
}

function imported(v: ModpackImportView): ModpackImportResult {
  return {
    instance: instanceSummary(v.instance),
    manualMods: v.manualMods,
    manualCount: v.manualCount,
    files: v.files,
    skipped: v.skipped,
    unsupported: v.unsupported,
  };
}

function preview(p: PackPreviewView): PackPreview {
  return {
    format: p.format,
    name: p.name,
    packVersion: p.packVersion ?? undefined,
    summary: p.summary ?? undefined,
    author: p.author ?? undefined,
    mcVersion: p.mcVersion,
    loader: p.loader,
    loaderVersion: p.loaderVersion ?? undefined,
    downloadFiles: p.downloadFiles,
    downloadBytes: p.downloadBytes,
    bundledFiles: p.bundledFiles,
    manualFiles: p.manualFiles,
    unsupported: p.unsupported,
    optional: p.optional.map((f) => ({ ...f, projectId: f.projectId ?? undefined })),
  };
}

function datapack(d: DatapackView): Datapack {
  return {
    fileName: d.fileName,
    name: d.name,
    description: d.description ?? undefined,
    sizeBytes: d.sizeBytes,
    folder: d.folder,
    icon: d.icon ?? undefined,
  };
}

const pickFilters: Record<PickKind, { name: string; extensions: string[] }> = {
  mod: { name: "Mod", extensions: ["jar", "zip", "litemod"] },
  resourcePack: { name: "Kaynak paketi", extensions: ["zip"] },
  shaderPack: { name: "Shader paketi", extensions: ["zip"] },
  modpack: { name: "Mod paketi", extensions: ["mrpack", "zip"] },
};

export const tauriContentApi: ContentApi = {
  async search(params) {
    const page = await ok(
      commands.modrinthSearch({
        projectType: params.projectType,
        query: params.query,
        sort: params.sort,
        category: params.category ?? null,
        offset: params.offset,
        limit: params.limit,
        instanceId: params.instanceId ?? null,
      }),
    );
    return { hits: page.hits.map(hit), offset: page.offset, total: page.totalHits };
  },
  async categories() {
    const list = await ok(commands.modrinthCategories());
    return list
      .filter((c) => (projectTypes as string[]).includes(c.projectType))
      .map((c) => ({
        name: c.name,
        projectType: c.projectType as ContentProjectType,
        header: c.header,
      }));
  },
  async project(id) {
    return project(await ok(commands.modrinthProject(id)));
  },
  async versions(projectId, instanceId, kind) {
    const list = await ok(commands.modrinthVersions(projectId, instanceId ?? null, kind ?? null));
    return list.map(version);
  },
  async scan(instanceId, kind, checkUpdates) {
    const scan = await ok(commands.contentScan(instanceId, kind, checkUpdates));
    return { entries: scan.entries.map(installed), offline: scan.offline };
  },
  async install(job, onProgress) {
    return report(
      await ok(
        commands.contentInstall(
          job.instanceId,
          job.kind,
          job.projectId,
          job.versionId ?? null,
          job.jobId,
          channel(onProgress),
        ),
      ),
    );
  },
  async update(job, onProgress) {
    return report(
      await ok(
        commands.contentUpdate(
          job.instanceId,
          job.kind,
          job.fileNames,
          job.jobId,
          channel(onProgress),
        ),
      ),
    );
  },
  async remove(instanceId, kind, fileName) {
    await ok(commands.contentRemove(instanceId, kind, fileName));
  },
  async addFiles(instanceId, kind, paths) {
    return ok(commands.contentAddFiles(instanceId, kind, paths));
  },
  async cancel(jobId) {
    await commands.contentCancel(jobId);
  },
  async previewPack(path) {
    return preview(await ok(commands.modpackPreview(path)));
  },
  async importPack(job, onProgress) {
    return imported(
      await ok(
        commands.modpackImport(
          {
            path: job.path,
            name: job.name ?? null,
            icon: job.icon,
            skipOptional: job.skipOptional ?? [],
          },
          job.jobId,
          channel(onProgress),
        ),
      ),
    );
  },
  async installModpack(job, onProgress) {
    return imported(
      await ok(
        commands.modpackInstall(
          job.projectId,
          job.versionId ?? null,
          job.name ?? null,
          job.icon,
          job.jobId,
          channel(onProgress),
        ),
      ),
    );
  },
  async exportPack(instanceId, options, jobId, onProgress) {
    return ok(
      commands.modpackExport(
        instanceId,
        { ...options, summary: options.summary ?? null },
        jobId,
        channel(onProgress),
      ),
    );
  },
  async modpackInfo(instanceId) {
    const info = await ok(commands.modpackInfo(instanceId));
    return info
      ? {
          projectId: info.projectId,
          versionId: info.versionId,
          versionNumber: info.versionNumber ?? undefined,
          title: info.title ?? undefined,
          iconUrl: info.iconUrl ?? undefined,
        }
      : null;
  },
  async modpackUpdateCheck(instanceId) {
    return (await ok(commands.modpackUpdateCheck(instanceId))) ?? null;
  },
  async modpackUpdatePlan(instanceId, versionId, jobId, onProgress) {
    const plan = await ok(
      commands.modpackUpdatePlan(instanceId, versionId, jobId, channel(onProgress)),
    );
    const change = (c: (typeof plan.added)[number]) => ({ ...c, iconUrl: c.iconUrl ?? undefined });
    return {
      fromVersion: plan.fromVersion ?? undefined,
      toVersion: plan.toVersion,
      added: plan.added.map(change),
      updated: plan.updated.map(change),
      removed: plan.removed.map(change),
      kept: plan.kept.map((k) => ({ item: change(k.item), reason: k.reason })),
      downloadBytes: plan.downloadBytes,
      loaderFrom: plan.loaderFrom ?? undefined,
      loaderTo: plan.loaderTo ?? undefined,
    };
  },
  modpackUpdateApply: (instanceId, versionId, jobId, onProgress) =>
    ok(commands.modpackUpdateApply(instanceId, versionId, jobId, channel(onProgress))),
  async projectBriefs(ids) {
    const list = await ok(commands.modrinthProjectBriefs(ids));
    return list.map((p) => ({ id: p.id, title: p.title, iconUrl: p.iconUrl ?? undefined }));
  },
  async datapacks(instanceId, world) {
    return (await ok(commands.datapacksList(instanceId, world))).map(datapack);
  },
  async installDatapack(job, onProgress) {
    return datapack(
      await ok(
        commands.datapackInstall(
          job.instanceId,
          job.world,
          job.projectId,
          job.jobId,
          channel(onProgress),
        ),
      ),
    );
  },
  async removeDatapack(instanceId, world, fileName) {
    await ok(commands.datapackRemove(instanceId, world, fileName));
  },
  async pickFiles(kind, multiple) {
    const chosen = await open({ multiple, directory: false, filters: [pickFilters[kind]] });
    if (chosen === null) return null;
    return Array.isArray(chosen) ? chosen : [chosen];
  },
  async pickSavePath(defaultName) {
    return save({
      defaultPath: defaultName,
      filters: [{ name: "Modrinth mod paketi", extensions: ["mrpack"] }],
    });
  },
};
