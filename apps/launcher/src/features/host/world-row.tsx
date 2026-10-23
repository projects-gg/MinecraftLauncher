// A world's row in the library's Worlds tab (HOST1, host1.md §1.3): "Arkadaşlarınla oyna" beside
// "Yedekle" (and in the row's "…" and right-click menus), only where the instance may share its
// worlds; nothing at all, and nothing said, elsewhere. While the world opens the slot says where it
// is ("Yedekleniyor %40" → "Oyun açılıyor" → "Dünyan açılıyor") with its bar; once open, the name
// gets "Dünyan açık" and the slot "Yönet". The slot keeps one width through every state, so the
// row never moves. One world at a time: the others wait, and say why.
import { UsersThree } from "@phosphor-icons/react";
import { Button, Chip, cn, type MenuItem, Progress, Tooltip } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { rise, useSocialMotion } from "@/features/social/motion";
import { useSocialUi } from "@/features/social/store";
import { hostApi } from "@/lib/api/host-api";
import type { WorldDetail } from "@/lib/api/library-types";
import type { InstanceDetail } from "@/lib/api/types";
import { formatPercent } from "@/lib/format";
import { type HostRowAction, hostRowAction } from "./rules";
import { useHostUi } from "./store";

/** The slot's width: the longest of its states ("Arkadaşlarınla oyna" with its icon). */
const SLOT = "w-[184px]";

export interface WorldShare {
  actionFor: (world: WorldDetail) => HostRowAction;
  /** "Arkadaşlarınla oyna" for the row's menus (null where the row offers nothing). */
  menuItem: (world: WorldDetail) => MenuItem | null;
  /** An opening of this instance's world makes its backup now: the tab's other jobs wait. */
  busy: boolean;
}

/** Opens the invite window for a world. */
function invite(instance: InstanceDetail, world: WorldDetail) {
  useHostUi.getState().openPicker({
    kind: "open",
    instanceId: instance.id,
    folder: world.folder,
    worldName: world.name,
    gameMode: world.gameMode,
    hardcore: world.hardcore,
  });
}

export function useWorldShare(instance: InstanceDetail, locked: boolean): WorldShare {
  const { t } = useTranslation();
  const view = useHostUi((s) => s.view);
  const signedIn = useSocialUi((s) => s.view.auth === "signedIn");
  const enabled = view.enabled && signedIn;
  // Asked again when the game starts or stops (the world it runs opens in place).
  const place = useQuery({
    queryKey: ["host-place", instance.id, locked],
    queryFn: () => hostApi.place(instance.id),
    enabled,
  });
  const actionFor = (world: WorldDetail) =>
    hostRowAction({
      enabled,
      place: place.data,
      session: view.session,
      instanceId: instance.id,
      folder: world.folder,
      locked,
      readable: world.readable,
    });
  const session = view.session;
  return {
    actionFor,
    busy:
      session?.phase === "opening" &&
      session.step === "backup" &&
      session.instanceId === instance.id,
    menuItem: (world) => {
      const action = actionFor(world);
      if (action.kind === "play") {
        return {
          label: t("host.row.play"),
          icon: <UsersThree size={16} />,
          disabled: action.wait !== undefined,
          onSelect: () => invite(instance, world),
        };
      }
      if (action.kind === "open" || action.kind === "failed") {
        return {
          label: t("host.row.manage"),
          icon: <UsersThree size={16} />,
          onSelect: () => useHostUi.getState().openSheet(),
        };
      }
      return null;
    },
  };
}

/** "Dünyan açık" beside the world's name while it is shared. */
export function WorldShareChip({ action }: { action: HostRowAction }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  return (
    <AnimatePresence initial={false}>
      {action.kind === "open" ? (
        <motion.span key="open" className="flex" {...rise(reduce, 0, 0.2)}>
          <Chip tone="jade">{t("host.row.open")}</Chip>
        </motion.span>
      ) : action.kind === "failed" ? (
        <motion.span key="failed" className="flex" {...rise(reduce, 0, 0.2)}>
          <Chip tone="flare">{t("host.row.failed")}</Chip>
        </motion.span>
      ) : null}
    </AnimatePresence>
  );
}

function Opening({ action }: { action: Extract<HostRowAction, { kind: "opening" }> }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const label = t(`host.row.opening.${action.step}`);
  const backup = action.step === "backup";
  return (
    <div className="w-full" role="status">
      <div className="mb-1 flex justify-between gap-2 text-[12px] text-ash-300">
        <span className="relative min-w-0 flex-1 overflow-hidden">
          <AnimatePresence initial={false} mode="popLayout">
            <motion.span key={action.step} className="block truncate" {...rise(reduce, 4, 0.2)}>
              {label}
            </motion.span>
          </AnimatePresence>
        </span>
        <span className="tabular-nums">
          {backup && action.percent !== undefined ? formatPercent(action.percent) : ""}
        </span>
      </div>
      <Progress value={backup ? (action.percent ?? 0) : undefined} label={label} />
    </div>
  );
}

/** The row's slot: "Arkadaşlarınla oyna", the opening's progress, or "Yönet". */
export function WorldShareAction({
  action,
  instance,
  world,
}: {
  action: HostRowAction;
  instance: InstanceDetail;
  world: WorldDetail;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const openSheet = useHostUi((s) => s.openSheet);
  if (action.kind === "hidden") return null;
  return (
    // Beside "Yedekle" (the slot hugs it from the right; the row's "…" stays at the end).
    <div className={cn("relative flex shrink-0 items-center justify-start", SLOT)}>
      <AnimatePresence initial={false} mode="popLayout">
        {action.kind === "play" ? (
          <motion.span key="play" className="flex" {...rise(reduce, 0, 0.2)}>
            <Tooltip content={t("host.row.busy")} disabled={action.wait !== "busy"}>
              <span className="inline-flex">
                <Button
                  size="sm"
                  variant="secondary"
                  icon={<UsersThree size={15} className="text-gold-300" />}
                  disabled={action.wait !== undefined}
                  onClick={() => invite(instance, world)}
                >
                  {t("host.row.play")}
                </Button>
              </span>
            </Tooltip>
          </motion.span>
        ) : action.kind === "opening" ? (
          <motion.span key="opening" className="flex w-full" {...rise(reduce, 0, 0.2)}>
            <Opening action={action} />
          </motion.span>
        ) : (
          <motion.span key="manage" className="flex" {...rise(reduce, 0, 0.2)}>
            <Button
              size="sm"
              variant="secondary"
              icon={
                <UsersThree
                  size={15}
                  className={action.kind === "failed" ? "text-flare-300" : "text-jade-400"}
                />
              }
              onClick={openSheet}
            >
              {t("host.row.manage")}
            </Button>
          </motion.span>
        )}
      </AnimatePresence>
    </div>
  );
}
