// The overview's side panel: whether the mods will start (with "Onar" when they will not), what
// the instance is (Minecraft, loader, room it takes) and quick ways to add content.
import {
  CheckCircle,
  DownloadSimple,
  Palette,
  PuzzlePiece,
  SunHorizon,
  WarningCircle,
  Wrench,
} from "@phosphor-icons/react";
import { Button, Skeleton } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { fade, swap } from "@/features/modcenter/motion";
import { useModCheck, useModFlow } from "@/features/modhealth/flow";
import { SeverityIcon } from "@/features/modhealth/issue-list";
import { fixKeysNow } from "@/features/modhealth/key-clash";
import { keyClashText } from "@/features/modhealth/key-texts";
import type { ContentKind, InstanceDetail } from "@/lib/api";
import { formatBytes, listFormat } from "@/lib/format";
import { incomingNames, useIncomingMods } from "../incoming-mods";

type Health = "noMods" | "incoming" | "pending" | "failed" | "ok" | "warnings" | "blocked";

/**
 * The mod check in one line: fine, only warnings, or the game would not start; "Onar" when a
 * repair has something to do, else "Göster" (the mod center lists the problems). Before the
 * first download, the pack's mods on their way. The second line keeps the button's height in
 * every state, so nothing moves when the answer arrives.
 */
function ModHealth({ instance, onShow }: { instance: InstanceDetail; onShow: () => void }) {
  const { t } = useTranslation();
  const openFlow = useModFlow((s) => s.open);
  const total = instance.mods.length;
  // Before the first download the pack's mods have not come yet (incoming-mods.ts): that is the
  // line, also over mods the player added meanwhile, and the check waits for them (it would
  // miss the libraries they bring). Asked only for such an instance; `[]` at once otherwise.
  const incoming = useIncomingMods(instance);
  const waiting = incoming === undefined || incoming.length > 0;
  const check = useModCheck(instance.id, total > 0 && !waiting);
  const data = check.data;
  const problems = (data?.issues ?? []).filter((issue) => issue.severity !== "info");
  // Only the pack's mod keys clash (KEYS2): the line names the keys and fixes them in one tap.
  const keyClash = problems.every((issue) => issue.kind === "keyClash") ? problems[0] : undefined;
  const queryClient = useQueryClient();
  const [fixingKeys, setFixingKeys] = useState(false);
  const health: Health =
    incoming === undefined
      ? "pending"
      : incoming.length > 0
        ? "incoming"
        : total === 0
          ? "noMods"
          : check.isError
            ? "failed"
            : !data
              ? "pending"
              : data.blocking > 0
                ? "blocked"
                : problems.length > 0
                  ? "warnings"
                  : "ok";

  const icon: Record<Health, ReactNode> = {
    noMods: <PuzzlePiece size={18} weight="duotone" className="text-ash-400" aria-hidden />,
    incoming: <DownloadSimple size={18} weight="duotone" className="text-gold-300" aria-hidden />,
    pending: <Skeleton className="size-[18px] rounded-full" />,
    failed: <WarningCircle size={18} className="text-ash-400" aria-hidden />,
    ok: <CheckCircle size={18} weight="fill" className="text-jade-400" aria-hidden />,
    warnings: <SeverityIcon severity="warning" />,
    blocked: <SeverityIcon severity="blocking" />,
  };
  const sentence: Record<Health, ReactNode> = {
    noMods: t("overview.health.noMods"),
    incoming: t("overview.incoming.title"),
    pending: <Skeleton className="h-3.5 w-32" />,
    failed: t("overview.health.failed"),
    ok: t("overview.health.ok"),
    warnings: keyClash ? keyClashText(t, keyClash.keys ?? []) : t("modHealth.banner.warnings"),
    blocked: t("modHealth.banner.blocked"),
  };

  // Under the sentence: which mods are on their way, or how many are on. While the pack is asked
  // the line is held for the answer that has one, so it does not push the facts down.
  let detail: { key: string; content: ReactNode } | null = null;
  if (incoming === undefined) {
    detail = { key: "asking", content: <Skeleton className="h-3 w-28" /> };
  } else if (incoming.length > 0) {
    const { names, more } = incomingNames(incoming);
    detail = {
      key: "incoming",
      content:
        more > 0
          ? t("overview.incoming.more", { name: names[0], count: more })
          : listFormat().format(names),
    };
  } else if (total > 0) {
    detail = {
      key: "summary",
      content: t("instance.mods.summary", {
        enabled: instance.mods.filter((mod) => mod.enabled).length,
        total,
      }),
    };
  }

  let action: ReactNode = null;
  if (health === "incoming") {
    action = (
      <Button size="sm" variant="ghost" className="-mr-1" onClick={onShow}>
        {t("overview.health.show")}
      </Button>
    );
  } else if (health === "warnings" && keyClash) {
    action = (
      <Button
        size="sm"
        variant="secondary"
        disabled={fixingKeys}
        onClick={() => {
          setFixingKeys(true);
          void fixKeysNow(t, queryClient, instance.id).finally(() => setFixingKeys(false));
        }}
      >
        {t("keyClash.fix")}
      </Button>
    );
  } else if ((health === "warnings" || health === "blocked") && data) {
    action = data.repairable ? (
      <Button
        size="sm"
        variant="secondary"
        icon={<Wrench size={15} weight="fill" className="text-gold-300" />}
        onClick={() => openFlow(instance.id, "center", { kind: "repair", scope: { kind: "all" } })}
      >
        {t("modHealth.banner.repair")}
      </Button>
    ) : (
      <Button size="sm" variant="ghost" className="-mr-1" onClick={onShow}>
        {t("overview.health.show")}
      </Button>
    );
  } else if (health === "failed") {
    action = (
      <Button size="sm" variant="ghost" className="-mr-1" onClick={() => void check.refetch()}>
        {t("common.retry")}
      </Button>
    );
  }

  return (
    <div className="flex gap-2.5" aria-busy={health === "pending" || undefined}>
      <span className="relative flex h-5 w-[18px] shrink-0 items-center justify-center">
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span key={health} {...swap()} className="flex">
            {icon[health]}
          </motion.span>
        </AnimatePresence>
      </span>
      <div className="min-w-0 flex-1">
        {/* Read out when the check answers (or a repair changes the answer). */}
        <p
          role="status"
          className="relative flex min-h-5 items-center text-[13.5px] leading-5 font-semibold text-bone-50"
        >
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span key={health} {...swap()} className="block">
              {sentence[health]}
            </motion.span>
          </AnimatePresence>
        </p>
        {detail ? (
          <div className="flex h-8 items-center justify-between gap-2">
            <span className="relative flex min-w-0 flex-1 items-center">
              <AnimatePresence mode="popLayout" initial={false}>
                <motion.span
                  key={detail.key}
                  {...swap()}
                  className="block truncate text-[12.5px] text-ash-300 tabular-nums"
                >
                  {detail.content}
                </motion.span>
              </AnimatePresence>
            </span>
            <AnimatePresence mode="popLayout" initial={false}>
              {action ? (
                <motion.span key={health} {...fade()} className="flex shrink-0">
                  {action}
                </motion.span>
              ) : null}
            </AnimatePresence>
          </div>
        ) : null}
      </div>
    </div>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="flex min-h-6 items-center justify-between gap-3">
      <dt className="shrink-0 text-[12.5px] text-ash-400">{label}</dt>
      <dd className="relative flex min-w-0 justify-end text-[13px] font-medium text-bone-100 tabular-nums">
        {children}
      </dd>
    </div>
  );
}

export function SidePanel({
  instance,
  sizeBytes,
  notDownloaded,
  onOpenModCenter,
  onBrowse,
}: {
  instance: InstanceDetail;
  /** The whole folder's size; null while it is first measured. */
  sizeBytes: number | null;
  /** Never downloaded and nothing put in it: "İndirilmedi" rather than a few hundred bytes. */
  notDownloaded: boolean;
  onOpenModCenter: () => void;
  onBrowse: (kind: ContentKind) => void;
}) {
  const { t } = useTranslation();
  const modded = instance.loader !== "vanilla";
  // Mods and shaders need a mod loader; resource packs work everywhere.
  const browse: Array<{ kind: ContentKind; icon: ReactNode }> = [
    ...(modded
      ? [
          { kind: "mod" as const, icon: <PuzzlePiece size={16} className="text-gold-300" /> },
          { kind: "shaderPack" as const, icon: <SunHorizon size={16} className="text-gold-300" /> },
        ]
      : []),
    { kind: "resourcePack", icon: <Palette size={16} className="text-gold-300" /> },
  ];
  return (
    <aside
      aria-label={t("overview.details")}
      className="glass space-y-4 rounded-[var(--radius-panel)] px-4 pt-3.5 pb-3"
    >
      {modded ? <ModHealth instance={instance} onShow={onOpenModCenter} /> : null}
      <dl>
        <Fact label={t("overview.facts.minecraft")}>{instance.mcVersion}</Fact>
        {modded && instance.loaderVersion ? (
          <Fact label={t(`loaders.${instance.loader}`)}>{instance.loaderVersion}</Fact>
        ) : null}
        <Fact label={t("overview.facts.size")}>
          <AnimatePresence mode="popLayout" initial={false}>
            {notDownloaded ? (
              <motion.span key="notDownloaded" {...swap()} className="truncate text-ash-300">
                {t("instances.needsInstall")}
              </motion.span>
            ) : sizeBytes === null ? (
              <motion.span key="pending" {...fade()} className="flex">
                <Skeleton className="h-3 w-14" />
              </motion.span>
            ) : (
              <motion.span key="size" {...swap()} className="truncate">
                {formatBytes(sizeBytes)}
              </motion.span>
            )}
          </AnimatePresence>
        </Fact>
      </dl>
      <div className="-mx-2 flex flex-col">
        {browse.map(({ kind, icon }) => (
          <Button
            key={kind}
            size="sm"
            variant="ghost"
            className="w-full justify-start px-2"
            icon={icon}
            onClick={() => onBrowse(kind)}
          >
            {t(`content.browse.title.${kind}`)}
          </Button>
        ))}
      </div>
    </aside>
  );
}
