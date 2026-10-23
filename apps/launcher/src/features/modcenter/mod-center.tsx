import { Plus, WarningCircle } from "@phosphor-icons/react";
import { Button, ContentIcon, EmptyState, Segmented, Skeleton } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { AnimatePresence, LayoutGroup, MotionConfig, motion } from "motion/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import offlineArt from "@/assets/art/empty-offline.webp";
import { ContentBrowser } from "@/features/content/content-browser";
import { ProjectDialog } from "@/features/content/project-dialog";
import { ModsTab, VanillaModsEmpty } from "@/features/library/tabs/mods-tab";
import { useModCheck } from "@/features/modhealth/flow";
import { ModHealthSection } from "@/features/modhealth/health-banner";
import { AddSetDialog } from "@/features/modsets/add-set-dialog";
import { InstanceSets, useInstanceSets } from "@/features/modsets/instance-sets";
import { api, type InstanceDetail, type ModCenterItem } from "@/lib/api";
import { useLaunch } from "@/stores/launch";
import { ModIssuesBanner } from "./issues-banner";
import { MigrateDialog } from "./migrate-dialog";
import { fade, glide, rise, useMotionPrefs } from "./motion";
import { RecommendedTab, refreshMods } from "./recommended-tab";

export type ModCenterSection = "recommended" | "sets" | "discover" | "own";

const titleWidths = ["w-40", "w-28", "w-36", "w-24", "w-32"];

/** Shaped like the recommended list: a category title, then rows (icon, two lines, switch). */
function Loading() {
  return (
    <div aria-busy className="pt-1">
      <Skeleton className="mx-3 mb-2 h-4 w-28" />
      {titleWidths.map((width) => (
        <div key={width} className="flex items-start gap-3 px-3 py-2.5">
          <Skeleton className="size-[36px] shrink-0" />
          <div className="flex-1 space-y-2 pt-0.5">
            <Skeleton className={`h-4 ${width}`} />
            <Skeleton className="h-3.5 w-3/5" />
          </div>
          <Skeleton className="mt-0.5 h-[20px] w-[34px] rounded-full" />
        </div>
      ))}
    </div>
  );
}

/**
 * The mod center of an instance: "Önerilenler" (the official pack's recommended mods, one by
 * one), "Keşfet" (Modrinth, filtered to the instance's version and loader) and "Kendi modların"
 * (the player's own mods). Problems with mod files show on top. Everything sits in one layout
 * group, so a banner opening or a notice leaving glides the content instead of jumping.
 */
export function ModCenter({
  instance,
  onSwitchVersion,
}: {
  instance: InstanceDetail;
  onSwitchVersion: () => void;
}) {
  const { t } = useTranslation();
  const prefs = useMotionPrefs();
  // The crash dialog's "Setlerde güncelle" opens the sets (FIX2); the request is then dropped
  // from the address.
  const { focus } = useSearch({ from: "/library/$instanceId" });
  const navigate = useNavigate();
  const [section, setSection] = useState<ModCenterSection>(() =>
    focus === "sets" ? "sets" : instance.managed ? "recommended" : "own",
  );
  useEffect(() => {
    if (focus !== "sets") return;
    setSection("sets");
    void navigate({
      to: "/library/$instanceId",
      params: { instanceId: instance.id },
      search: (prev) => ({ ...prev, focus: undefined }),
      replace: true,
    });
  }, [focus, instance.id, navigate]);
  const [project, setProject] = useState<ModCenterItem | null>(null);
  const [migrating, setMigrating] = useState(false);
  const [addingSet, setAddingSet] = useState(false);
  const queryClient = useQueryClient();
  // The instance's ready-made sets (the "Setler" section and "Set ekle" share the query).
  const instanceSets = useInstanceSets(instance.id, instance.loader !== "vanilla");
  const addedSets = new Set((instanceSets.data?.sets ?? []).map((set) => set.id));
  const view = useQuery({
    queryKey: ["mod-center", instance.id],
    queryFn: () => api.modCenter.view(instance.id),
    staleTime: 15_000,
    refetchOnMount: "always",
    retry: false,
  });
  // The mod check (what would stop the game), refreshed whenever the mods change.
  const health = useModCheck(instance.id, instance.loader !== "vanilla");
  // A launch syncs the pack (updates, adopted changes): show the result once the game starts.
  const launchPhase = useLaunch((s) => (s.instanceId === instance.id ? s.phase.kind : null));
  useEffect(() => {
    if (launchPhase === "launching" || launchPhase === "running") {
      refreshMods(queryClient, instance.id);
    }
  }, [launchPhase, queryClient, instance.id]);

  if (instance.loader === "vanilla") return <VanillaModsEmpty />;

  const newCount = view.data?.items.filter((i) => i.isNew && !i.library).length ?? 0;
  const sections: Array<{ value: ModCenterSection; label: string }> = [
    ...(instance.managed
      ? [
          {
            value: "recommended" as const,
            label:
              newCount > 0
                ? t("modCenter.sections.recommendedNew", { count: newCount })
                : t("modCenter.sections.recommended"),
          },
        ]
      : []),
    {
      value: "sets",
      label:
        addedSets.size > 0
          ? t("modSets.center.sectionCount", { count: addedSets.size })
          : t("modSets.center.section"),
    },
    { value: "discover", label: t("modCenter.sections.discover") },
    { value: "own", label: t("modCenter.sections.own") },
  ];
  const pack = section === "recommended" ? view.data?.pack : undefined;
  const position = prefs.layout ? ("position" as const) : false;
  // Files the sync already lists as unreadable are not repeated by the mod check.
  const unreadable = new Set((view.data?.issues ?? []).map((i) => i.fileName.toLowerCase()));
  const healthIssues = (health.data?.issues ?? []).filter(
    (i) => !(i.kind === "unreadable" && i.files.some((f) => unreadable.has(f.toLowerCase()))),
  );
  const healthSection =
    health.data && healthIssues.length > 0 ? (
      <ModHealthSection instanceId={instance.id} check={{ ...health.data, issues: healthIssues }} />
    ) : undefined;

  let content = null;
  if (section === "recommended") {
    if (view.isPending) {
      content = <Loading />;
    } else if (view.isError || !view.data) {
      content = (
        <EmptyState
          icon={<ContentIcon kind="failed" />}
          title={t("modCenter.loadFailed")}
          body={view.error instanceof Error ? view.error.message : undefined}
          action={
            <Button variant="secondary" onClick={() => void view.refetch()}>
              {t("common.retry")}
            </Button>
          }
        />
      );
    } else if (view.data.dropped) {
      const dropped = view.data.dropped;
      content = (
        <motion.div
          {...rise(prefs.reduce)}
          className="glass flex items-start gap-3 rounded-[var(--radius-panel)] px-4 py-3.5"
        >
          <WarningCircle size={20} className="mt-0.5 shrink-0 text-gold-300" />
          <div className="min-w-0 flex-1">
            <p className="text-[14px] font-semibold text-bone-50">{t("modCenter.dropped.title")}</p>
            <p className="mt-0.5 text-[13px] text-ash-300">
              {t("modCenter.dropped.body", { minecraft: dropped.minecraft })}
              {dropped.recommended
                ? ` ${t("modCenter.dropped.recommended", { version: dropped.recommended })}`
                : ""}
            </p>
          </div>
          <Button size="sm" variant="secondary" onClick={onSwitchVersion}>
            {t("modCenter.dropped.action")}
          </Button>
        </motion.div>
      );
    } else if (view.data.unavailable) {
      content = (
        <EmptyState
          illustration={offlineArt}
          title={t("modCenter.unavailable.title")}
          body={t("modCenter.unavailable.body")}
          action={
            <Button variant="secondary" onClick={() => void view.refetch()}>
              {t("common.retry")}
            </Button>
          }
        />
      );
    } else {
      content = (
        <RecommendedTab
          view={view.data}
          onOpenProject={(item) => item.projectId && setProject(item)}
        />
      );
    }
  } else if (section === "sets") {
    content = <InstanceSets instance={instance} onAdd={() => setAddingSet(true)} />;
  } else if (section === "discover") {
    content = <ContentBrowser instance={instance} kind="mod" embedded />;
  } else {
    content = (
      <div className="pb-8">
        <ModsTab
          instance={instance}
          ownOnly
          onBrowse={() => setSection("discover")}
          onMigrate={() => setMigrating(true)}
        />
      </div>
    );
  }

  return (
    <MotionConfig reducedMotion="user">
      <LayoutGroup id={`mod-center-${instance.id}`}>
        <div className="flex min-h-0 max-w-[980px] flex-1 flex-col">
          {view.data || healthSection ? (
            <ModIssuesBanner issues={view.data?.issues ?? []} reset={view.data?.reset ?? false}>
              {healthSection}
            </ModIssuesBanner>
          ) : null}
          <motion.div
            layout={position}
            transition={{ layout: glide }}
            className="mb-4 flex shrink-0 flex-wrap items-center gap-3"
          >
            <Segmented
              value={section}
              onValueChange={setSection}
              label={t("modCenter.sectionsLabel")}
              options={sections}
            />
            <div className="ml-auto flex items-center gap-3">
              <AnimatePresence initial={false}>
                {pack ? (
                  <motion.span
                    key="pack"
                    {...fade(0.16)}
                    className="text-[12.5px] text-ash-400 tabular-nums"
                  >
                    {t("modCenter.packCaption", {
                      name: pack.name,
                      version: pack.version,
                      minecraft: pack.minecraft,
                    })}
                  </motion.span>
                ) : null}
              </AnimatePresence>
              <Button
                size="sm"
                variant="secondary"
                icon={<Plus size={14} weight="bold" />}
                onClick={() => setAddingSet(true)}
              >
                {t("modSets.center.add")}
              </Button>
            </div>
          </motion.div>

          {/* Sections swap with a quick fade: switching tabs is frequent, so no movement. */}
          <AnimatePresence mode="wait" initial={false}>
            <motion.div
              key={section}
              {...fade(0.16)}
              layout={position}
              transition={{ layout: glide }}
              className="flex min-h-0 flex-1 flex-col"
            >
              {content}
            </motion.div>
          </AnimatePresence>
        </div>
      </LayoutGroup>
      <ProjectDialog
        projectId={project?.projectId ?? null}
        hint={
          project
            ? {
                title: project.title,
                author: "",
                iconUrl: project.iconUrl,
                description: project.summary ?? "",
              }
            : undefined
        }
        onClose={() => setProject(null)}
      />
      <MigrateDialog instance={instance} open={migrating} onClose={() => setMigrating(false)} />
      <AddSetDialog
        instance={instance}
        open={addingSet}
        addedIds={addedSets}
        onClose={() => setAddingSet(false)}
        // The added set shows where its items are switched.
        onAdded={() => setSection("sets")}
      />
    </MotionConfig>
  );
}
