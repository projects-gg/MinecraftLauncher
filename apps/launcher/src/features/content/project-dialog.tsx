import {
  ArrowSquareOut,
  BookOpen,
  Bug,
  CaretLeft,
  CaretRight,
  ChatsCircle,
  ClockCounterClockwise,
  Code,
  DownloadSimple,
  Heart,
  UsersThree,
  X,
} from "@phosphor-icons/react";
import { Button, Chip, Dialog, IconButton, Skeleton, TabPanel, Tabs } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  api,
  type ContentGalleryImage,
  type ContentHit,
  type ContentKind,
  type ContentLink,
  type ContentProject,
  type ContentVersion,
} from "@/lib/api";
import { formatBytes, formatRelative } from "@/lib/format";
import { isWebLink, openWebLink } from "@/lib/links";
import { ProjectDescription } from "./description";
import { projectJobKey, useContentActions, useContentJobs } from "./jobs";
import { categoryLabel, formatCompact, InstallControl, installState, ProjectIcon } from "./parts";
import { useContentScan } from "./use-content";

type DetailTab = "description" | "gallery" | "versions";

const linkIcons: Record<ContentLink["kind"], ReactNode> = {
  source: <Code size={15} />,
  issues: <Bug size={15} />,
  wiki: <BookOpen size={15} />,
  discord: <ChatsCircle size={15} />,
  donate: <Heart size={15} />,
};

export function modrinthUrl(project: { id: string; slug?: string }, type: string) {
  const path = type === "other" ? "project" : type;
  return `https://modrinth.com/${path}/${project.slug ?? project.id}`;
}

function Gallery({ images }: { images: ContentGalleryImage[] }) {
  const { t } = useTranslation();
  const [index, setIndex] = useState<number | null>(null);
  if (images.length === 0) return <p className="text-ash-400">{t("content.detail.noGallery")}</p>;
  const current = index === null ? undefined : images[index];
  if (index !== null && current) {
    const step = (delta: number) => setIndex((index + delta + images.length) % images.length);
    return (
      <figure>
        <div className="relative flex min-h-[220px] items-center justify-center overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-950/70">
          <img
            src={current.fullUrl ?? current.url}
            alt={current.title ?? ""}
            className="max-h-[52vh] w-auto max-w-full object-contain"
            draggable={false}
          />
          <div className="absolute top-2 right-2">
            <IconButton
              label={t("content.detail.galleryClose")}
              icon={<X size={16} />}
              size="sm"
              className="bg-obsidian-950/70"
              onClick={() => setIndex(null)}
            />
          </div>
        </div>
        <div className="mt-3 flex items-start gap-3">
          <figcaption className="min-w-0 flex-1">
            {current.title ? (
              <p className="text-[14px] font-semibold text-bone-50">{current.title}</p>
            ) : null}
            {current.description ? (
              <p className="mt-0.5 text-[13px] text-ash-400">{current.description}</p>
            ) : null}
          </figcaption>
          <span className="pt-1 text-[12px] text-ash-400 tabular-nums">
            {index + 1} / {images.length}
          </span>
          <IconButton
            label={t("content.detail.galleryPrev")}
            icon={<CaretLeft size={16} />}
            size="sm"
            onClick={() => step(-1)}
          />
          <IconButton
            label={t("content.detail.galleryNext")}
            icon={<CaretRight size={16} />}
            size="sm"
            onClick={() => step(1)}
          />
        </div>
      </figure>
    );
  }
  return (
    <ul className="grid grid-cols-2 gap-3">
      {images.map((image, i) => (
        <li key={image.url}>
          <button
            type="button"
            onClick={() => setIndex(i)}
            className="group block w-full cursor-zoom-in overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-800 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
          >
            <img
              src={image.url}
              alt={image.title ?? ""}
              loading="lazy"
              draggable={false}
              className="aspect-video w-full object-cover transition-transform duration-300 group-hover:scale-[1.02] motion-reduce:group-hover:scale-100"
            />
          </button>
          {image.title ? (
            <p className="mt-1.5 truncate text-[13px] font-medium text-bone-100">{image.title}</p>
          ) : null}
        </li>
      ))}
    </ul>
  );
}

function VersionRow({
  version,
  installedVersionId,
  busy,
  onInstall,
}: {
  version: ContentVersion;
  installedVersionId?: string;
  busy: boolean;
  onInstall?: () => void;
}) {
  const { t } = useTranslation();
  const installed = installedVersionId === version.id;
  return (
    <li className="flex items-center gap-3 rounded-[var(--radius-panel)] px-3 py-2.5 transition-colors hover:bg-white/5">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-[14px] font-semibold text-bone-50">
            {version.versionNumber}
          </span>
          {version.versionType !== "release" ? (
            <Chip>{t(`content.detail.versionTypes.${version.versionType}`)}</Chip>
          ) : null}
        </div>
        <p className="mt-0.5 flex flex-wrap gap-x-4 text-[12px] text-ash-400">
          <span>{formatRelative(version.datePublished)}</span>
          <span className="tabular-nums">
            {t("content.browse.downloads", { count: formatCompact(version.downloads) })}
          </span>
          <span className="truncate">
            {version.gameVersions.slice(-3).join(", ")}
            {version.loaders.length ? ` (${version.loaders.join(", ")})` : ""}
          </span>
          {version.fileSize ? (
            <span className="tabular-nums">{formatBytes(version.fileSize)}</span>
          ) : null}
        </p>
      </div>
      {installed ? (
        <Chip tone="jade">{t("content.detail.current")}</Chip>
      ) : onInstall ? (
        <Button
          size="sm"
          variant="ghost"
          disabled={busy}
          icon={<DownloadSimple size={15} />}
          onClick={onInstall}
        >
          {t("content.detail.installThis")}
        </Button>
      ) : null}
    </li>
  );
}

function Versions({
  project,
  instanceId,
  kind,
  installedVersionId,
  busy,
  onInstall,
}: {
  project: ContentProject;
  instanceId?: string;
  kind?: ContentKind;
  installedVersionId?: string;
  busy: boolean;
  onInstall?: (versionId: string) => void;
}) {
  const { t } = useTranslation();
  const { data, isPending, isError, refetch } = useQuery({
    queryKey: ["content-versions", project.id, instanceId ?? null, kind ?? null],
    queryFn: () => api.content.versions(project.id, instanceId, kind),
    staleTime: 5 * 60_000,
  });
  if (isPending) {
    return (
      <div className="space-y-2">
        {[0, 1, 2, 3].map((i) => (
          <Skeleton key={i} className="h-14 w-full" />
        ))}
      </div>
    );
  }
  if (isError) {
    return (
      <div className="flex items-center gap-3 text-ash-400">
        {t("content.detail.error")}
        <Button size="sm" variant="ghost" onClick={() => void refetch()}>
          {t("common.retry")}
        </Button>
      </div>
    );
  }
  if (data.length === 0) return <p className="text-ash-400">{t("content.detail.noVersions")}</p>;
  return (
    <ul className="space-y-0.5">
      {data.slice(0, 60).map((v) => (
        <VersionRow
          key={v.id}
          version={v}
          installedVersionId={installedVersionId}
          busy={busy}
          onInstall={onInstall ? () => onInstall(v.id) : undefined}
        />
      ))}
    </ul>
  );
}

/** Where installs from the dialog go. Without it the dialog only shows the project. */
export interface InstallTarget {
  instanceId: string;
  kind: ContentKind;
}

/**
 * Project page from Modrinth: description (sanitized), gallery and versions, with the
 * install/update/remove control for the instance it was opened from.
 */
export function ProjectDialog({
  projectId,
  hint,
  target,
  onClose,
  footer,
}: {
  projectId: string | null;
  hint?: Pick<ContentHit, "title" | "author" | "iconUrl" | "description">;
  target?: InstallTarget;
  onClose: () => void;
  /** Replaces the install control (e.g. "choose this modpack"). */
  footer?: ReactNode;
}) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<DetailTab>("description");
  useEffect(() => {
    if (projectId) setTab("description");
  }, [projectId]);

  const {
    data: project,
    isPending,
    isError,
    refetch,
  } = useQuery({
    queryKey: ["content-project", projectId],
    queryFn: () => api.content.project(projectId ?? ""),
    enabled: Boolean(projectId),
    staleTime: 10 * 60_000,
  });
  // Only while open: the mods tab checks for updates on demand.
  const scan = useContentScan(
    target?.instanceId,
    target?.kind ?? "mod",
    Boolean(target) && Boolean(projectId),
  );
  const installed = scan.data?.entries.find((e) => e.projectId === projectId);
  const actions = useContentActions(target?.instanceId ?? "", target?.kind ?? "mod");
  const jobKey = projectJobKey(target?.instanceId ?? "", projectId ?? "");
  const job = useContentJobs((s) => s.jobs[jobKey]);

  const title = project?.title ?? hint?.title ?? "";
  const iconUrl = project?.iconUrl ?? hint?.iconUrl;

  const control =
    footer ??
    (target && projectId ? (
      <InstallControl
        state={installState(installed)}
        job={job}
        onInstall={() => void actions.install(projectId, title)}
        onUpdate={() => installed && void actions.update([installed])}
        onRemove={() => {
          if (installed) void actions.remove(installed.fileName, title);
        }}
        onCancel={() => actions.cancel(jobKey)}
      />
    ) : null);

  return (
    <Dialog
      open={Boolean(projectId)}
      onOpenChange={(open) => !open && onClose()}
      title={title}
      description={hint?.author ? t("content.browse.by", { author: hint.author }) : undefined}
      closeLabel={t("common.close")}
      width={880}
      // One height from the first frame: the panel used to open at the skeleton's height and
      // jump twice as the page and its description arrived (453 → 381 → 563 px at 1024 × 640),
      // and again on every tab change. Now only what is inside it changes; it scrolls (POLISH6).
      className="h-[min(600px,88vh)]"
      footer={
        <>
          {project ? (
            <Button
              variant="ghost"
              icon={<ArrowSquareOut size={16} />}
              onClick={() => openWebLink(modrinthUrl(project, project.projectType))}
            >
              {t("content.actions.openModrinth")}
            </Button>
          ) : null}
          <span className="flex-1" />
          {control}
        </>
      }
    >
      <div className="flex items-start gap-4">
        <ProjectIcon src={iconUrl} size={72} />
        <div className="min-w-0 flex-1">
          <p className="text-[14px] leading-relaxed text-ash-300">
            {project?.description ?? hint?.description}
          </p>
          {project ? (
            <>
              <dl className="mt-3 flex flex-wrap gap-x-6 gap-y-2 text-[12.5px]">
                {[
                  [
                    <DownloadSimple key="d" size={15} />,
                    t("content.detail.downloads"),
                    formatCompact(project.downloads),
                  ],
                  [
                    <UsersThree key="f" size={15} />,
                    t("content.detail.followers"),
                    formatCompact(project.followers),
                  ],
                  [
                    <ClockCounterClockwise key="u" size={15} />,
                    t("content.detail.updated"),
                    formatRelative(project.updated),
                  ],
                  ...(project.license
                    ? [
                        [
                          <Code key="l" size={15} />,
                          t("content.detail.license"),
                          project.license.replace(/^LicenseRef-/, ""),
                        ],
                      ]
                    : []),
                ].map(([icon, label, value]) => (
                  <div key={String(label)} className="flex items-center gap-1.5">
                    <span className="text-ash-400">{icon}</span>
                    <dt className="text-ash-400">{label}</dt>
                    <dd className="font-semibold text-bone-100 tabular-nums">{value}</dd>
                  </div>
                ))}
              </dl>
              {project.categories.length ? (
                <div className="mt-3 flex flex-wrap gap-1.5">
                  {project.categories.slice(0, 8).map((c) => (
                    <Chip key={c}>{categoryLabel(t, c)}</Chip>
                  ))}
                </div>
              ) : null}
            </>
          ) : null}
        </div>
      </div>

      {/* The author's links (source, wiki, Discord, donations): https:// pages only. */}
      {project?.links.some((link) => isWebLink(link.url)) ? (
        <div className="mt-4 flex flex-wrap gap-1">
          {project.links
            .filter((link) => isWebLink(link.url))
            .map((link) => (
              <Button
                key={link.kind}
                size="sm"
                variant="ghost"
                icon={linkIcons[link.kind]}
                onClick={() => openWebLink(link.url)}
              >
                {t(`content.detail.links.${link.kind}`)}
              </Button>
            ))}
        </div>
      ) : null}

      {isPending && projectId ? (
        <div className="mt-6 space-y-3">
          <Skeleton className="h-10 w-full" />
          <Skeleton className="h-4 w-11/12" />
          <Skeleton className="h-4 w-10/12" />
          <Skeleton className="h-40 w-full" />
        </div>
      ) : isError ? (
        <div className="mt-6 flex items-center gap-3 text-ash-400">
          {t("content.detail.error")}
          <Button size="sm" variant="ghost" onClick={() => void refetch()}>
            {t("common.retry")}
          </Button>
        </div>
      ) : project ? (
        <Tabs
          value={tab}
          onValueChange={setTab}
          label={title}
          className="mt-4"
          items={[
            { value: "description", label: t("content.detail.tabs.description") },
            {
              value: "gallery",
              label: t("content.detail.tabs.gallery"),
              count: project.gallery.length || undefined,
            },
            { value: "versions", label: t("content.detail.tabs.versions") },
          ]}
        >
          <TabPanel value="description" className="overflow-visible">
            <ProjectDescription body={project.body || project.description} />
          </TabPanel>
          <TabPanel value="gallery" className="overflow-visible">
            <Gallery images={project.gallery} />
          </TabPanel>
          <TabPanel value="versions" className="overflow-visible">
            <Versions
              project={project}
              instanceId={target?.instanceId}
              kind={target?.kind}
              installedVersionId={installed?.versionId}
              busy={Boolean(job)}
              onInstall={
                target
                  ? (versionId) => void actions.install(project.id, title, versionId)
                  : undefined
              }
            />
          </TabPanel>
        </Tabs>
      ) : null}
    </Dialog>
  );
}
