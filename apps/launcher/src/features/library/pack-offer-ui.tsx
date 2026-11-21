// PACK-OFFER: the official pack where the player picks a Minecraft version by hand, and the
// family's newer versions told once on the official instance. Shaped like the mod center's notices
// (the pack's egg, one sentence, what it does, one button); nothing installs or moves until the
// player presses it. The version switch keeps its own dialog: this only opens it on the version.
import { Stack, X } from "@phosphor-icons/react";
import { Button, cn, IconButton, SwapLabel, toast, useMotionPrefs } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { useNavigate } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { fade, rise } from "@/features/modcenter/motion";
import type { InstanceSummary } from "@/lib/api";
import { InstanceIcon } from "@/shell/instance-icon";
import { usePrefs } from "@/stores/prefs";
import { useInstallOfficial } from "./official-ui";
import {
  newerMembers,
  offerSeenKey,
  officialFamilyQuery,
  type PackOffer,
  packOffer,
} from "./pack-offer";

type Surface = "dialog" | "page";

const surfaces: Record<Surface, string> = {
  // Inside a dialog: the plate the official pack's row has in "Resmî paketi kuralım mı?".
  dialog: "bg-obsidian-950/45 ring-1 ring-white/7 ring-inset",
  // On a tab: the mod center's notices.
  page: "glass",
};

function Row({
  surface,
  icon,
  title,
  body,
  action,
  onDismiss,
}: {
  surface: Surface;
  icon: React.ReactNode;
  title: string;
  body?: string;
  action: React.ReactNode;
  onDismiss?: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div
      role="status"
      className={cn(
        "flex items-start gap-2 rounded-[var(--radius-panel)] px-4 py-3",
        surfaces[surface],
      )}
    >
      {/* Narrow windows: the button goes under the sentence instead of squeezing it; "Gizle"
          stays in the corner. */}
      <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-3 gap-y-2.5">
        <span className="flex shrink-0">{icon}</span>
        <div className="min-w-[min(15rem,100%)] flex-1">
          <p className="text-balance text-[14px] font-semibold text-bone-50">{title}</p>
          {body ? <p className="mt-0.5 text-[13px] text-ash-300">{body}</p> : null}
        </div>
        {action}
      </div>
      {onDismiss ? (
        <IconButton
          size="sm"
          label={t("packOffer.dismiss")}
          icon={<X size={14} />}
          onClick={onDismiss}
        />
      ) : null}
    </div>
  );
}

/**
 * The offer for a version picked by hand: install the official pack on it, move the official
 * instance to it (its version dialog, aimed at the version) or open it when it is there already.
 * `onLeave` runs before the page changes (the new-instance dialog closes).
 */
export function PackOfferNotice({
  offer,
  surface,
  onLeave,
  onDismiss,
}: {
  offer: PackOffer;
  surface: Surface;
  onLeave?: () => void;
  onDismiss?: () => void;
}) {
  const { t } = useTranslation();
  const navigate = useNavigate();
  const install = useInstallOfficial();
  const { pack } = offer;
  const vars = { name: pack.name, minecraft: pack.mcVersion };
  const goTo = (instanceId: string, switchTo?: string) => {
    onLeave?.();
    usePrefs.getState().selectInstance(instanceId);
    void navigate({
      to: "/library/$instanceId",
      params: { instanceId },
      search: switchTo ? { tab: "mods", switchTo } : {},
    });
  };
  const press = () => {
    if (offer.kind === "open") goTo(offer.instanceId);
    else if (offer.kind === "switch") goTo(offer.instanceId, pack.mcVersion);
    else if (!install.isPending) {
      install.mutate(pack.mcVersion, {
        onSuccess: (made) => {
          toast.success(t("packOffer.installed", { name: made.name }));
          goTo(made.id);
        },
      });
    }
  };
  const body =
    offer.kind === "install"
      ? t("packOffer.installBody", vars)
      : offer.kind === "switch"
        ? t("packOffer.switchBody", vars)
        : t("packOffer.openBody", vars);
  const label =
    offer.kind === "install"
      ? t("packOffer.install")
      : offer.kind === "switch"
        ? t("packOffer.switch", vars)
        : t("packOffer.open");
  return (
    <Row
      surface={surface}
      icon={<InstanceIcon icon="dragon-egg" size={surface === "dialog" ? 36 : 32} />}
      title={t("packOffer.title")}
      body={body}
      onDismiss={onDismiss}
      action={
        <Button
          size="sm"
          variant="secondary"
          aria-busy={install.isPending || undefined}
          onClick={press}
        >
          {offer.kind === "install" ? (
            <SwapLabel labels={[label, t("lib1.installing")]} active={install.isPending ? 1 : 0} />
          ) : (
            label
          )}
        </Button>
      }
    />
  );
}

/**
 * A custom instance with no mods on a version the official pack has: the offer, until the player
 * hides it (remembered for this instance and pack) or adds a mod.
 */
export function EmptyInstanceOffer({
  instance,
  empty,
}: {
  instance: InstanceSummary;
  empty: boolean;
}) {
  const seen = usePrefs((s) => s.packOffersSeen);
  const enabled = !instance.managed && instance.loader !== "vanilla" && empty;
  const family = useQuery({ ...officialFamilyQuery(instance.mcVersion), enabled });
  const offer = enabled
    ? packOffer(
        family.data,
        { minecraft: instance.mcVersion, loader: instance.loader },
        instance.id,
      )
    : null;
  const key = offer ? offerSeenKey(instance.id, offer.pack.id) : "";
  const shown = offer && !seen.includes(key) ? offer : null;
  return (
    <OfferPresence show={Boolean(shown)}>
      {shown ? (
        <div className="mb-4">
          <PackOfferNotice
            offer={shown}
            surface="page"
            onDismiss={() => usePrefs.getState().set("packOffersSeen", [...seen, key])}
          />
        </div>
      ) : null}
    </OfferPresence>
  );
}

/** "Bu sürüm için hazır setler var." with "Setlere Bak" (the new-instance dialog's sets source). */
export function SetsHint({ onOpen }: { onOpen: () => void }) {
  const { t } = useTranslation();
  return (
    <Row
      surface="dialog"
      icon={<Stack size={20} className="text-ash-300" />}
      title={t("packOffer.sets")}
      action={
        <Button size="sm" variant="secondary" onClick={onOpen}>
          {t("packOffer.setsAction")}
        </Button>
      }
    />
  );
}

/** Presence for the notices: they come and go as the picked version changes. */
export function OfferPresence({ show, children }: { show: boolean; children: React.ReactNode }) {
  const { reduce } = useMotionPrefs();
  return (
    <AnimatePresence initial={false}>
      {show ? (
        <motion.div key="offer" {...(reduce ? fade() : rise(false))}>
          {children}
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}

/**
 * An instance of the official family on an older version: the newest member it was not told
 * about, once ("{{name}} artık Minecraft X için de var." with "Sürümü Değiştir"). Hiding it, or
 * moving to that version, remembers every newer member, so a later one is told again.
 */
export function NewerMemberNotice({
  instance,
  onSwitch,
}: {
  instance: InstanceSummary;
  onSwitch: (minecraft: string) => void;
}) {
  const { t } = useTranslation();
  const seen = usePrefs((s) => s.packOffersSeen);
  const family = useQuery({
    ...officialFamilyQuery(instance.mcVersion),
    enabled: instance.managed,
  });
  const newer = instance.managed ? newerMembers(family.data, instance.mcVersion, seen) : [];
  const top = newer[0];
  const remember = () =>
    usePrefs.getState().set("packOffersSeen", [...seen, ...newer.map((m) => m.packId)]);
  return (
    <OfferPresence show={Boolean(top)}>
      {top ? (
        <div className="mb-4">
          <Row
            surface="page"
            icon={<InstanceIcon icon="dragon-egg" size={32} />}
            title={t("packOffer.newer", { minecraft: top.minecraft })}
            onDismiss={remember}
            action={
              <Button
                size="sm"
                variant="secondary"
                onClick={() => {
                  remember();
                  onSwitch(top.minecraft);
                }}
              >
                {t("packOffer.switch", { minecraft: top.minecraft })}
              </Button>
            }
          />
        </div>
      ) : null}
    </OfferPresence>
  );
}
