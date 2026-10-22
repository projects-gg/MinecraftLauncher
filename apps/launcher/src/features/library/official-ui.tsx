// LIB1: the official pack once the player can delete it. The library's empty state, the quiet
// "Kurulu değil" card where the pack was, and the offer an action opens when it needed the pack
// (OYNA with nothing to play, "Katıl", the tray's "Oyna"). Every one of them installs the pack only
// when the player presses its button; nothing comes back by itself (the backend remembers).
import { Plus } from "@phosphor-icons/react";
import {
  Button,
  Chip,
  Dialog,
  EmptyState,
  easeOutExpo,
  Skeleton,
  SwapLabel,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { motion } from "motion/react";
import type { Ref } from "react";
import { useTranslation } from "react-i18next";
import emptyArt from "@/assets/art/empty-library.webp";
import { showError } from "@/features/content/jobs";
import { joinProjects, joinServer } from "@/features/social/join";
import { api, type InstanceSummary, type OfficialPack, type OfficialStatus } from "@/lib/api";
import { InstanceIcon } from "@/shell/instance-icon";
import { useLaunch } from "@/stores/launch";
import { usePrefs } from "@/stores/prefs";
import { officialStatusQuery, playTarget, useOfficialOffer } from "./official";

/** The official instance's name before the status names its pack. */
const PROJECTS_NAME = "Projects";
/** Where "Katıl" goes: the Projects server. */
const PROJECTS_HOST = "play.projects.gg";
/** Where a card rests (the instance cards' own entrance and exit move from and to it). */
const REST = "translateY(0px) scale(1)";

/**
 * "Resmî paketi kur": the pack's instance is made now (its files download at the first play), it
 * becomes the one OYNA starts, and the lists show it at once, so the offer leaves as its card comes.
 */
export function useInstallOfficial() {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: () => api.installOfficialInstance(),
    onSuccess: (made) => {
      queryClient.setQueryData<InstanceSummary[]>(["instances"], (prev) =>
        prev && !prev.some((i) => i.id === made.id) ? [...prev, made] : prev,
      );
      queryClient.setQueryData<OfficialStatus>(["official-status"], (prev) =>
        prev ? { ...prev, instanceId: made.id, removed: false } : prev,
      );
      usePrefs.getState().selectInstance(made.id);
      void queryClient.invalidateQueries({ queryKey: ["instances"] });
      void queryClient.invalidateQueries({ queryKey: ["official-status"] });
    },
    onError: (error) => showError(error),
  });
}

/** The pack's line under its name, as an instance card writes it ("Fabric 26.1.2"). */
function packLine(pack: OfficialPack, loaderName: string): string {
  return `${pack.loader === "vanilla" ? "Minecraft" : loaderName} ${pack.mcVersion}`.trim();
}

/**
 * The library with no instance left: the chest, "Hiç kurulum yok", "Resmî paketi kuralım mı?",
 * then "Resmî paketi kur" and "Yeni kurulum". Out of the list's flow and centred in the visible
 * area; it fades in after the last card has gone (a page that opens on it simply shows it).
 */
export function LibraryEmpty({ onCreate }: { onCreate: () => void }) {
  const { t } = useTranslation();
  const install = useInstallOfficial();
  const { reduce } = useMotionPrefs();
  const installing = install.isPending;
  return (
    <motion.div
      initial={{ opacity: 0, transform: reduce ? "translateY(0px)" : "translateY(8px)" }}
      animate={{
        opacity: 1,
        transform: "translateY(0px)",
        transition: { duration: 0.34, ease: easeOutExpo, delay: 0.12 },
      }}
      exit={{ opacity: 0, transition: { duration: 0.14, ease: easeOutExpo } }}
      className="absolute inset-0 flex items-center justify-center pb-[6vh]"
    >
      <EmptyState
        illustration={emptyArt}
        title={t("lib1.empty.title")}
        body={t("lib1.empty.question")}
        action={
          <div className="flex flex-wrap items-center justify-center gap-2.5">
            <Button
              variant="primary"
              aria-busy={installing || undefined}
              onClick={() => {
                if (installing) return;
                install.mutate(undefined, {
                  onSuccess: (made) => toast.success(t("official.added", { name: made.name })),
                });
              }}
            >
              <SwapLabel
                labels={[t("lib1.install"), t("lib1.installing")]}
                active={installing ? 1 : 0}
              />
            </Button>
            <Button
              variant="secondary"
              icon={<Plus size={16} weight="bold" className="text-gold-300" />}
              onClick={onCreate}
            >
              {t("library.create")}
            </Button>
          </div>
        }
      />
    </motion.div>
  );
}

/**
 * Where the official pack was, when only it is missing: a quiet card in "Resmî paketler" with the
 * pack's dimmed mark, its name, "Kurulu değil" and "Kur" (the button always shows: it is the card's
 * one action). No glass and no hover lift: it reads as an empty place, not as an instance. Enters
 * and leaves as the instance cards do, so installing cross-fades it into the new card in place.
 */
export function NotInstalledCard({
  pack,
  index,
  ref,
}: {
  pack: OfficialPack | undefined;
  index: number;
  /** Set by the list's AnimatePresence (popLayout measures the leaving card). */
  ref?: Ref<HTMLElement>;
}) {
  const { t } = useTranslation();
  const { layout, reduce } = useMotionPrefs();
  const install = useInstallOfficial();
  const installing = install.isPending;
  const name = pack?.name ?? PROJECTS_NAME;
  return (
    <motion.article
      ref={ref}
      layout={layout ? "position" : false}
      initial={{ opacity: 0, transform: reduce ? REST : "translateY(12px) scale(1)" }}
      animate={{ opacity: 1, transform: REST }}
      exit={{
        opacity: 0,
        transform: reduce ? REST : "translateY(0px) scale(0.97)",
        transition: { duration: 0.18, ease: easeOutExpo },
      }}
      transition={{ duration: 0.4, delay: Math.min(index, 8) * 0.035, ease: easeOutExpo }}
      className="group relative isolate flex flex-col rounded-[var(--radius-panel)] bg-obsidian-900/35 p-3.5 ring-1 ring-white/8 transition-[box-shadow,background-color] duration-200 ring-inset hover:bg-obsidian-900/50 hover:ring-white/14"
    >
      <InstanceIcon
        icon="dragon-egg"
        size={48}
        className="opacity-50 saturate-[0.55] transition-[opacity,filter] duration-300 group-focus-within:opacity-90 group-focus-within:saturate-100 group-hover:opacity-90 group-hover:saturate-100"
      />
      <h3 className="mt-3 truncate font-display text-[15px] font-semibold text-bone-50/85">
        {name}
      </h3>
      <p className="mt-0.5 truncate text-[12.5px] text-ash-300">{t("lib1.notInstalled")}</p>
      <Button
        variant="secondary"
        size="sm"
        className="mt-2.5 w-full"
        aria-label={t("lib1.install")}
        aria-busy={installing || undefined}
        onClick={() => {
          if (installing) return;
          install.mutate(undefined, {
            onSuccess: (made) => toast.success(t("official.added", { name: made.name })),
          });
        }}
      >
        <SwapLabel
          labels={[t("lib1.installShort"), t("lib1.installing")]}
          active={installing ? 1 : 0}
        />
      </Button>
    </motion.article>
  );
}

/** A long instance name in a button: cut so the verb stays ("Arkadaşlarla Survi… ile katıl"). */
function shortName(name: string, max = 22): string {
  return name.length > max ? `${name.slice(0, max - 1).trimEnd()}…` : name;
}

/**
 * The offer an action opens when it needed the official pack and the player had deleted it
 * (`officialOrOffer`): "Hiç kurulum yok" (or "Resmî paket kurulu değil" when other instances are
 * there), the question, the pack it would install, and "Kur ve oyna" / "Kur ve katıl", which go on
 * with what the player started. The other button: "Yeni kurulum" with nothing to play, or joining
 * with the instance OYNA would start. Mounted once in the app shell.
 */
export function OfficialOfferDialog() {
  const { t } = useTranslation();
  const intent = useOfficialOffer((s) => s.intent);
  const close = useOfficialOffer((s) => s.close);
  const navigate = useNavigate();
  const open = intent !== null;
  const { data: instances = [] } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
    enabled: open,
  });
  const { data: official, isPending: statusPending } = useQuery({
    ...officialStatusQuery,
    enabled: open,
  });
  const selectedId = usePrefs((s) => s.selectedInstanceId);
  const install = useInstallOfficial();
  const installing = install.isPending;
  // Joining needs no new instance: the one OYNA would start can go instead.
  const instead = intent && intent.kind !== "play" ? playTarget(instances, selectedId) : undefined;
  const pack = official?.pack;

  const installAndGo = () => {
    if (!intent || installing) return;
    install.mutate(undefined, {
      onSuccess: (made) => {
        close();
        if (intent.kind === "play") void useLaunch.getState().play(made.id);
        else if (intent.kind === "join") void joinProjects();
        else void joinServer({ host: intent.host, port: intent.port });
      },
    });
  };
  const other = () => {
    if (!intent) return;
    close();
    if (!instead) {
      void navigate({ to: "/library", search: { create: true } });
      return;
    }
    usePrefs.getState().selectInstance(instead.id);
    void joinServer(
      intent.kind === "joinServer"
        ? { host: intent.host, port: intent.port }
        : { host: PROJECTS_HOST, port: null },
    );
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && close()}
      title={instances.length > 0 ? t("lib1.offer.missingTitle") : t("lib1.empty.title")}
      description={t("lib1.empty.question")}
      closeLabel={t("common.close")}
      width={440}
      footer={
        <>
          <Button variant="secondary" onClick={other}>
            {instead
              ? t("lib1.offer.joinWith", { name: shortName(instead.name) })
              : t("lib1.offer.create")}
          </Button>
          <Button variant="primary" aria-busy={installing || undefined} onClick={installAndGo}>
            <SwapLabel
              labels={[
                intent?.kind === "play" ? t("lib1.offer.play") : t("lib1.offer.join"),
                t("lib1.installing"),
              ]}
              active={installing ? 1 : 0}
            />
          </Button>
        </>
      }
    >
      {/* What "Kur" brings: the pack, as the welcome flow shows it. */}
      <div className="flex items-center gap-3 rounded-[var(--radius-panel)] bg-obsidian-950/45 px-3 py-2.5 ring-1 ring-white/7 ring-inset">
        <InstanceIcon icon="dragon-egg" size={40} />
        <div className="min-w-0 flex-1">
          <p className="truncate font-display text-[15px] font-semibold text-bone-50">
            {pack?.name ?? PROJECTS_NAME}
          </p>
          {pack ? (
            <p className="truncate text-[12.5px] text-ash-300">
              {packLine(pack, t(`loaders.${pack.loader}`))}
            </p>
          ) : statusPending ? (
            <Skeleton className="mt-1 h-3 w-24" />
          ) : null}
        </div>
        <Chip tone="gold">{t("instances.managed")}</Chip>
      </div>
    </Dialog>
  );
}
