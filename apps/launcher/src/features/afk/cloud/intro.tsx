// "7/24 Bulut" before the player has a cloud account: one quiet card on the AFK page and, behind
// it, a short panel (what it does, the price from /offer, "afk.projects.gg'de başla"). Purchase and
// linking always happen on the site. The panel's three points rise in once; nothing moves after.
import { ArrowSquareOut, CaretRight } from "@phosphor-icons/react";
import {
  Button,
  ContentIcon,
  type ContentIconKind,
  cn,
  Dialog,
  easeOutExpo,
  useMotionPrefs,
} from "@pl/ui";
import { motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { useSocialUi } from "@/features/social/store";
import { afkFailed } from "../errors";
import { type CloudOverview, cloudApi } from "./api";
import { formatPrice } from "./status";

/**
 * The price line ("Aylık 60 TL"), or why nothing can be bought now: no free slot, or sales off
 * (Studio's switch; SMALL12: the card showed the price while nothing was on sale). `closed` greys
 * the line and turns the panel's start off.
 */
function useOfferLine(overview: CloudOverview): { text: string; closed: boolean } | null {
  const { t } = useTranslation();
  if (overview.soldOut) return { text: t("cloudAfk.intro.soldOut"), closed: true };
  if (!overview.salesOpen) return { text: t("cloudAfk.intro.salesClosed"), closed: true };
  if (!overview.monthPrice) return null;
  return {
    text: t("cloudAfk.intro.pricePerMonth", { price: formatPrice(overview.monthPrice) }),
    closed: false,
  };
}

/** The card on the AFK page; the whole card opens the panel. */
export function CloudIntroCard({
  overview,
  onOpen,
  className,
}: {
  overview: CloudOverview;
  onOpen: () => void;
  className?: string;
}) {
  const { t } = useTranslation();
  const offer = useOfferLine(overview);
  return (
    <button
      type="button"
      onClick={onOpen}
      className={cn(
        "glass group flex w-full cursor-pointer items-center gap-3.5 rounded-[var(--radius-panel)] p-3.5 text-left outline-none",
        "shadow-[inset_0_0_0_1px_rgb(255_255_255/0.06)] transition-[box-shadow,transform,scale] duration-150 ease-out",
        "hover:shadow-[inset_0_0_0_1px_rgb(246_213_142/0.28)] focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.985]",
        className,
      )}
    >
      <span className="chamfer grid size-[52px] shrink-0 place-items-center bg-obsidian-800 [--cut:8px]">
        <ContentIcon kind="cloud" size={32} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block font-display text-[15px] leading-tight font-semibold text-bone-50">
          {t("cloudAfk.title")}
        </span>
        <span className="mt-0.5 block truncate text-[12.5px] text-ash-300">
          {t("cloudAfk.intro.subtitle")}
        </span>
      </span>
      {offer ? (
        <span
          className={cn(
            "shrink-0 text-[12.5px] whitespace-nowrap tabular-nums",
            offer.closed ? "text-ash-300" : "font-display text-[13px] font-semibold text-gold-200",
          )}
        >
          {offer.text}
        </span>
      ) : null}
      <CaretRight
        size={16}
        weight="bold"
        aria-hidden
        className="shrink-0 text-ash-400 transition-[color,transform] duration-150 ease-out group-hover:translate-x-0.5 group-hover:text-bone-100 motion-reduce:group-hover:translate-x-0"
      />
    </button>
  );
}

const POINTS: Array<{ key: "online" | "commands" | "anywhere"; icon: ContentIconKind }> = [
  { key: "online", icon: "server" },
  { key: "commands", icon: "logs" },
  { key: "anywhere", icon: "afk" },
];

/** The panel behind the card: what 7/24 Bulut does and where it starts. */
export function CloudIntroDialog({
  overview,
  open,
  onOpenChange,
}: {
  overview: CloudOverview;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const offer = useOfferLine(overview);
  const signedOut = overview.session === "signedOut";
  // With the inventory live (CAFK-FEAT §6.4) the third point names it; the count stays three.
  const inventoryLive = (overview.features?.inventory ?? "off") !== "off";
  const inventoryPoint = (point: (typeof POINTS)[number]) =>
    inventoryLive && point.key === "anywhere";
  const start = () =>
    cloudApi
      .open("start")
      .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")));
  const signIn = () => {
    onOpenChange(false);
    useSocialUi.getState().openPanel();
  };
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("cloudAfk.title")}
      description={t("cloudAfk.intro.subtitle")}
      closeLabel={t("common.close")}
      width={520}
      footer={
        <>
          {offer ? (
            <span
              className={cn(
                "mr-auto text-[13px] tabular-nums",
                offer.closed ? "text-ash-300" : "font-display font-semibold text-gold-200",
              )}
            >
              {offer.text}
            </span>
          ) : null}
          <Button
            variant="primary"
            icon={<ArrowSquareOut size={16} weight="bold" />}
            disabled={offer?.closed}
            onClick={() => void start()}
          >
            {t("cloudAfk.intro.start")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-4 pt-1">
        <div className="flex items-start gap-4">
          <div className="chamfer grid size-[72px] shrink-0 place-items-center bg-obsidian-800 [--cut:10px]">
            <ContentIcon kind="cloud" size={48} />
          </div>
          <ul className="flex min-w-0 flex-1 flex-col gap-2.5">
            {POINTS.map((point, index) => (
              <motion.li
                key={point.key}
                className="flex items-center gap-3 text-[13.5px] text-bone-100"
                initial={{ opacity: 0, transform: reduce ? "none" : "translateY(6px)" }}
                animate={{
                  opacity: 1,
                  transform: reduce ? "none" : "translateY(0px)",
                  transition: { duration: 0.32, delay: 0.12 + index * 0.05, ease: easeOutExpo },
                }}
              >
                <span className="grid size-8 shrink-0 place-items-center rounded-md bg-white/[0.04]">
                  <ContentIcon kind={inventoryPoint(point) ? "inventory" : point.icon} size={16} />
                </span>
                <span className="min-w-0">
                  {inventoryPoint(point)
                    ? t("cloudAfkFeat.intro.anywhere")
                    : t(`cloudAfk.intro.points.${point.key}`)}
                </span>
              </motion.li>
            ))}
          </ul>
        </div>
        {signedOut ? (
          <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[12.5px] text-ash-300">
            <span>{t("cloudAfk.intro.signIn")}</span>
            <button
              type="button"
              onClick={signIn}
              className="cursor-pointer rounded-sm font-medium text-gold-300 underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
            >
              {t("cloudAfk.intro.signInAction")}
            </button>
          </p>
        ) : null}
      </div>
    </Dialog>
  );
}
