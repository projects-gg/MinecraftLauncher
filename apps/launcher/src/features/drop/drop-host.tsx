// Files dragged anywhere onto the window. As soon as a drag enters, the backend reads what the
// files are and a layer shows where they will go: the open instance page's instance, else the
// one selected in the command bar. Mods go into its active set, resource and shader packs into
// their folders, a modpack opens the import flow; unknown files get one short notice. Places
// with their own drop handling (skin panel, the import step) claim drops and this layer steps
// aside (`useDropClaim`).
import {
  Package,
  Palette,
  PuzzlePiece,
  Question,
  SunHorizon,
  WarningCircle,
} from "@phosphor-icons/react";
import { cn, easeOutExpo, toast, usePerf } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate, useRouterState } from "@tanstack/react-router";
import { AnimatePresence, motion, useReducedMotion, type Variants } from "motion/react";
import { type ReactNode, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api, type DroppedFile, type InstanceSummary } from "@/lib/api";
import { playSound } from "@/lib/sound";
import { InstanceIcon } from "@/shell/instance-icon";
import { usePrefs } from "@/stores/prefs";
import {
  type DropNotice,
  type DropPlan,
  type DropRow,
  type DropTarget,
  dropNotices,
  planDrop,
  type RowKind,
  stem,
} from "./model";
import { useDrop } from "./store";

const rowIcons: Record<RowKind, typeof PuzzlePiece> = {
  mod: PuzzlePiece,
  resourcePack: Palette,
  shaderPack: SunHorizon,
  modpack: Package,
  unknown: Question,
};

/** The drag in progress, with its files once the backend has read them. */
interface Drag {
  id: number;
  files: DroppedFile[] | null;
}

/** How the layer leaves: the files were taken, or the drag ended without them. */
type Outcome = "taken" | "gone";

const toTarget = (instance: InstanceSummary | undefined): DropTarget | null =>
  instance ? { id: instance.id, modded: instance.loader !== "vanilla" } : null;

/** The instance a drop goes into: the open instance page, else the command bar's choice. */
function useDropTarget(): InstanceSummary | undefined {
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const selectedId = usePrefs((s) => s.selectedInstanceId);
  const { data: instances = [] } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
  });
  const match = /^\/library\/([^/]+)$/.exec(pathname)?.[1];
  const pageId = match ? decodeURIComponent(match) : undefined;
  // The open page's own query (cached by the page) covers an instance the list does not know yet.
  const { data: page } = useQuery({
    queryKey: ["instance", pageId],
    queryFn: () => api.instance(pageId ?? ""),
    enabled: Boolean(pageId),
  });
  return useMemo(() => {
    const onPage = pageId ? (instances.find((i) => i.id === pageId) ?? page) : undefined;
    return onPage ?? instances.find((i) => i.id === selectedId) ?? instances[0];
  }, [instances, page, pageId, selectedId]);
}

/** Layer, corners and card share these labels; exits read the outcome (AnimatePresence custom). */
function layerVariants(): Variants {
  return {
    hidden: { opacity: 0 },
    shown: { opacity: 1, transition: { duration: 0.18, ease: easeOutExpo } },
    exit: (outcome: Outcome) => ({
      opacity: 0,
      transition:
        outcome === "taken"
          ? { duration: 0.24, ease: easeOutExpo, delay: 0.06 }
          : { duration: 0.14, ease: easeOutExpo },
    }),
  };
}

function cardVariants(reduce: boolean): Variants {
  const t = (value: string) => (reduce ? "none" : value);
  return {
    hidden: { opacity: 0, transform: t("translateY(10px) scale(0.98)") },
    shown: {
      opacity: 1,
      transform: t("translateY(0px) scale(1)"),
      transition: { duration: 0.28, ease: easeOutExpo },
    },
    // Taken: the card sinks into the window; dropped elsewhere or cancelled: it simply goes.
    exit: (outcome: Outcome) =>
      outcome === "taken"
        ? {
            opacity: 0,
            transform: t("translateY(0px) scale(0.94)"),
            transition: { duration: 0.2, ease: easeOutExpo },
          }
        : {
            opacity: 0,
            transform: t("translateY(6px) scale(0.98)"),
            transition: { duration: 0.14, ease: easeOutExpo },
          },
  };
}

function cornerVariants(dx: number, dy: number, reduce: boolean): Variants {
  const shift = (px: number) => (reduce ? "none" : `translate(${dx * px}px, ${dy * px}px)`);
  return {
    hidden: { opacity: 0, transform: shift(14) },
    shown: { opacity: 1, transform: shift(0), transition: { duration: 0.34, ease: easeOutExpo } },
    // Taken: the brackets close in a little more as they fade, the window "takes" the files.
    exit: (outcome: Outcome) => ({
      opacity: 0,
      transform: shift(outcome === "taken" ? -8 : 10),
      transition: { duration: outcome === "taken" ? 0.24 : 0.14, ease: easeOutExpo },
    }),
  };
}

const cornerPlace = {
  tl: "top-4 left-4",
  tr: "top-4 right-4",
  bl: "bottom-4 left-4",
  br: "right-4 bottom-4",
} as const;

/** A pixel L-bracket in a window corner (brackets mark the target, docs/design.md §1). */
function Corner({
  position,
  accept,
  reduce,
}: {
  position: keyof typeof cornerPlace;
  accept: boolean;
  reduce: boolean;
}) {
  const dx = position.endsWith("l") ? -1 : 1;
  const dy = position.startsWith("t") ? -1 : 1;
  const tone = accept ? "bg-gold-300" : "bg-ash-500";
  return (
    <motion.span
      aria-hidden
      className={cn("absolute size-7", cornerPlace[position])}
      variants={cornerVariants(dx, dy, reduce)}
    >
      <span
        className={cn(
          "absolute h-[3px] w-full transition-colors duration-200",
          tone,
          dy < 0 ? "top-0" : "bottom-0",
        )}
      />
      <span
        className={cn(
          "absolute h-full w-[3px] transition-colors duration-200",
          tone,
          dx < 0 ? "left-0" : "right-0",
        )}
      />
    </motion.span>
  );
}

function Row({ row, index, reduce }: { row: DropRow; index: number; reduce: boolean }) {
  const { t } = useTranslation();
  const Icon = rowIcons[row.kind];
  const muted = row.kind === "unknown" || Boolean(row.blocked);
  return (
    <motion.li
      className="flex items-center gap-3 py-2"
      initial={{ opacity: 0, transform: reduce ? "none" : "translateY(6px)" }}
      animate={{
        opacity: 1,
        transform: reduce ? "none" : "translateY(0px)",
        transition: { duration: 0.26, ease: easeOutExpo, delay: 0.04 + index * 0.035 },
      }}
    >
      <span
        className={cn(
          "flex size-8 shrink-0 items-center justify-center rounded-md",
          muted ? "bg-white/5 text-ash-400" : "bg-gold-500/14 text-gold-300",
        )}
      >
        <Icon size={18} weight={muted ? "regular" : "duotone"} />
      </span>
      <span className="min-w-0 flex-1">
        <span
          className={cn(
            "block truncate text-[14px] font-medium",
            muted ? "text-ash-400" : "text-bone-50",
          )}
        >
          {t(`drop.kinds.${row.kind}`)}
        </span>
        {row.blocked ? (
          <span className="block truncate text-[12px] text-flare-400">
            {t(`drop.blocked.${row.blocked}`)}
          </span>
        ) : null}
      </span>
      <span
        className={cn(
          "font-display text-[15px] font-semibold tabular-nums",
          muted ? "text-ash-400" : "text-bone-100",
        )}
      >
        {row.count}
      </span>
    </motion.li>
  );
}

function Header({ icon, title, subtitle }: { icon: ReactNode; title: string; subtitle: string }) {
  return (
    <div className="flex items-center gap-3.5">
      {icon}
      <div className="min-w-0">
        <p className="truncate font-display text-[18px] leading-tight font-semibold text-bone-50">
          {title}
        </p>
        <p className="mt-0.5 truncate text-[13px] text-ash-400">{subtitle}</p>
      </div>
    </div>
  );
}

function Tile({ tone, children }: { tone: "gold" | "ash"; children: ReactNode }) {
  return (
    <span
      className={cn(
        "chamfer flex size-12 shrink-0 items-center justify-center [--cut:7px]",
        tone === "gold"
          ? "bg-[radial-gradient(circle_at_50%_35%,rgb(232_154_44/0.3),rgb(27_24_32/0.95)_70%)] text-gold-300"
          : "bg-obsidian-800 text-ash-400",
      )}
    >
      {children}
    </span>
  );
}

function Card({
  plan,
  target,
  files,
  reduce,
}: {
  plan: DropPlan | null;
  target: InstanceSummary | undefined;
  files: DroppedFile[] | null;
  reduce: boolean;
}) {
  const { t } = useTranslation();
  const [slow, setSlow] = useState(false);
  // Reading the files usually takes milliseconds: placeholders only when it does not.
  useEffect(() => {
    if (files) return;
    const timer = setTimeout(() => setSlow(true), 140);
    return () => clearTimeout(timer);
  }, [files]);

  const count = files?.length ?? 0;
  const onlyUnknown = plan?.rows.every((r) => r.kind === "unknown") ?? false;
  let header: ReactNode;
  if (plan?.importOnly && plan.modpack) {
    header = (
      <Header
        icon={
          <Tile tone="gold">
            <Package size={26} weight="duotone" />
          </Tile>
        }
        title={t("drop.newInstance")}
        subtitle={stem(plan.modpack.name)}
      />
    );
  } else if (onlyUnknown) {
    const first = files?.[0];
    header = (
      <Header
        icon={
          <Tile tone="ash">
            <WarningCircle size={26} />
          </Tile>
        }
        title={t("drop.kinds.unknown")}
        subtitle={count === 1 && first ? first.name : t("drop.files", { count })}
      />
    );
  } else if (target) {
    const loader = target.loader === "vanilla" ? "Minecraft" : t(`loaders.${target.loader}`);
    header = (
      <Header
        icon={<InstanceIcon icon={target.icon} size={48} />}
        title={target.name}
        subtitle={`${loader} ${target.mcVersion}`}
      />
    );
  } else {
    header = (
      <Header
        icon={
          <Tile tone="ash">
            <Package size={26} />
          </Tile>
        }
        title={t("drop.noInstanceTitle")}
        subtitle={t("drop.files", { count })}
      />
    );
  }

  const rows = plan && !plan.importOnly && !onlyUnknown ? plan.rows : [];
  return (
    <div className="glass-strong w-[360px] rounded-[var(--radius-panel)] p-4 shadow-[0_30px_80px_rgb(34_6_6/0.6)]">
      {header}
      {rows.length > 0 ? (
        <ul className="mt-3 border-t border-white/8 pt-1.5">
          {rows.map((row, index) => (
            <Row key={row.kind} row={row} index={index} reduce={reduce} />
          ))}
        </ul>
      ) : !files && slow ? (
        <div className="mt-3 space-y-2.5 border-t border-white/8 pt-3" aria-hidden>
          <div className="h-8 rounded-md bg-white/5" />
          <div className="h-8 w-4/5 rounded-md bg-white/5" />
        </div>
      ) : null}
    </div>
  );
}

/** The window-wide drop layer (mounted once in the shell). */
export function DropHost({ disabled = false }: { disabled?: boolean }) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const target = useDropTarget();
  const reduce = useReducedMotion() ?? false;
  const { performanceMode } = usePerf();
  const [drag, setDrag] = useState<Drag | null>(null);
  const [outcome, setOutcome] = useState<Outcome>("gone");
  const next = useRef(0);
  const current = useRef<Drag | null>(null);
  const live = useRef({ target, disabled });
  live.current = { target, disabled };

  useEffect(() => {
    const show = (value: Drag | null) => {
      current.current = value;
      setDrag(value);
    };

    const announce = (into: InstanceSummary, notices: DropNotice[]) => {
      // The newest toast sits in front: the most important one goes last.
      for (const notice of [...notices].reverse()) {
        const message = t(`drop.notices.${notice.key}`, notice.values);
        const tab = notice.show;
        const action = tab
          ? {
              label: t("drop.show"),
              onClick: () =>
                void navigate({
                  to: "/library/$instanceId",
                  params: { instanceId: into.id },
                  search: { tab },
                }),
            }
          : undefined;
        if (notice.tone === "success") toast.success(message, { action });
        else if (notice.tone === "error") toast.error(message);
        else toast(message);
      }
    };

    const finish = async (paths: string[], known: DroppedFile[] | null) => {
      const into = live.current.target;
      const files = known ?? (await api.classifyFiles(paths).catch(() => [] as DroppedFile[]));
      const modpack = files.find((f) => f.kind === "modpack");
      const copies = files.some(
        (f) => f.kind === "mod" || f.kind === "resourcePack" || f.kind === "shaderPack",
      );
      if (copies && into) {
        try {
          const result = await api.dropFiles(into.id, paths);
          announce(into, dropNotices(result));
          if (result.added.length > 0) {
            playSound("success");
            void queryClient.invalidateQueries({ queryKey: ["instance", into.id] });
            void queryClient.invalidateQueries({ queryKey: ["content-scan", into.id] });
          }
        } catch (error) {
          toast.error(
            error instanceof Error && error.message
              ? error.message
              : t("drop.notices.failedMany", { count: paths.length }),
          );
        }
      } else {
        const unknown = files.filter((f) => f.kind === "unknown");
        const first = unknown[0];
        if (unknown.length === 1 && first) {
          toast(t("drop.notices.unknownOne", { name: first.name }));
        } else if (unknown.length > 1) {
          toast(t("drop.notices.unknownMany", { count: unknown.length }));
        }
        if (copies && !into) toast(t("drop.blocked.noInstance"));
      }
      if (modpack) {
        useDrop.getState().setPendingPack(modpack.path);
        void navigate({ to: "/library", search: { create: true } });
      }
    };

    return api.onFileDrag((event) => {
      if (live.current.disabled || useDrop.getState().claims > 0) {
        if (current.current) show(null);
        return;
      }
      switch (event.type) {
        case "enter": {
          // Text or links dragged from a browser carry no files: nothing to show.
          if (event.paths.length === 0) return;
          next.current += 1;
          const id = next.current;
          setOutcome("gone");
          show({ id, files: null });
          void api
            .classifyFiles(event.paths)
            .catch(() => [] as DroppedFile[])
            .then((files) => {
              if (current.current?.id === id) show({ id, files });
            });
          break;
        }
        case "drop": {
          const known = current.current?.files ?? null;
          const plan = known ? planDrop(known, toTarget(live.current.target)) : null;
          setOutcome(plan?.acceptable ? "taken" : "gone");
          show(null);
          if (event.paths.length > 0) void finish(event.paths, known);
          break;
        }
        case "leave":
          setOutcome("gone");
          show(null);
          break;
        default:
          break;
      }
    });
  }, [navigate, queryClient, t]);

  const plan = useMemo(
    () => (drag?.files ? planDrop(drag.files, toTarget(target)) : null),
    [drag?.files, target],
  );
  // Until the files are read the target looks ready; brackets turn grey for files it refuses.
  const accept = plan ? plan.acceptable : true;

  return (
    <AnimatePresence custom={outcome}>
      {drag ? (
        <motion.div
          key="drop"
          role="status"
          aria-live="polite"
          className="pointer-events-none fixed inset-0 z-[70] flex items-center justify-center"
          initial="hidden"
          animate="shown"
          exit="exit"
          variants={layerVariants()}
        >
          <div className="absolute inset-0 bg-obsidian-950/72" />
          {performanceMode ? null : (
            <div className="absolute inset-0 bg-[radial-gradient(560px_circle_at_50%_50%,rgb(232_154_44/0.09),transparent_70%)]" />
          )}
          {(["tl", "tr", "bl", "br"] as const).map((position) => (
            <Corner key={position} position={position} accept={accept} reduce={reduce} />
          ))}
          <motion.div className="relative" variants={cardVariants(reduce)}>
            <Card key={drag.id} plan={plan} target={target} files={drag.files} reduce={reduce} />
          </motion.div>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
