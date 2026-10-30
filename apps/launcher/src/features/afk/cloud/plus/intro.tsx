// "AFK+" for a player without an AFK+ account: a quiet card beside 7/24 Bulut's on the AFK page
// and, behind it, a short panel that shows the product instead of describing it: the conversation
// the assistant has for the player (a real haggle, each line with who says it: the other player's
// name, "ProBot AI"; never the [AFK+] tag, which is off by default), what it never does in one
// sentence, the price from the offer and "afk.projects.gg'de başla". Purchase happens on the site.
// The conversation's lines come in once, one after another; nothing moves after. Prices are bone:
// the one gold is the start button (design.md §2).
import { ArrowSquareOut, CaretRight } from "@phosphor-icons/react";
import { Button, ContentIcon, cn, Dialog, easeOutExpo, useMotionPrefs } from "@pl/ui";
import { motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { useSocialUi } from "@/features/social/store";
import { afkFailed } from "../../errors";
import { type CloudOverview, cloudApi } from "../api";
import { formatPrice } from "../status";
import { plusOffer } from "./types";

/** "Aylık 200 TL", or why nothing can be bought now. */
function useOfferLine(overview: CloudOverview): { text: string; open: boolean } | null {
  const { t } = useTranslation();
  const plus = plusOffer(overview);
  if (!plus) return null;
  if (!plus.sales) return { text: t("afkPlus.intro.closed"), open: false };
  if (plus.stock.soldOut) return { text: t("afkPlus.intro.soldOut"), open: false };
  const month = plus.prices.find((price) => price.plan === "month");
  if (!month) return null;
  return {
    text: t("afkPlus.intro.pricePerMonth", { price: formatPrice(month.priceTl) }),
    open: true,
  };
}

export function PlusIntroCard({
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
        <ContentIcon kind="chat" size={32} />
      </span>
      <span className="min-w-0 flex-1">
        <span className="block font-display text-[15px] leading-tight font-semibold text-bone-50">
          {t("afkPlus.name")}
        </span>
        <span className="mt-0.5 line-clamp-2 block text-[12.5px] text-pretty text-ash-300">
          {t("afkPlus.intro.subtitle")}
        </span>
      </span>
      {offer ? (
        <span
          className={cn(
            "shrink-0 text-[12.5px] whitespace-nowrap tabular-nums",
            offer.open ? "font-display text-[13px] font-semibold text-bone-50" : "text-ash-300",
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

/** The haggle the panel shows: who says what, each line after its speaker's name. */
const CHAT: Array<{ from: "them" | "bot"; key: string }> = [
  { from: "them", key: "c1" },
  { from: "bot", key: "c2" },
  { from: "them", key: "c3" },
  { from: "bot", key: "c4" },
  { from: "them", key: "c5" },
];

export function PlusIntroDialog({
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
  const start = () =>
    cloudApi.open("plus").catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")));
  const signIn = () => {
    onOpenChange(false);
    useSocialUi.getState().openPanel();
  };
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("afkPlus.name")}
      description={t("afkPlus.intro.subtitle")}
      closeLabel={t("common.close")}
      width={520}
      footer={
        <>
          {offer ? (
            <span
              className={cn(
                "mr-auto text-[13px] tabular-nums",
                offer.open ? "font-display font-semibold text-bone-50" : "text-ash-300",
              )}
            >
              {offer.text}
            </span>
          ) : null}
          <Button
            variant="primary"
            icon={<ArrowSquareOut size={16} weight="bold" />}
            disabled={offer ? !offer.open : false}
            onClick={() => void start()}
          >
            {t("afkPlus.intro.start")}
          </Button>
        </>
      }
    >
      <div className="flex flex-col gap-3.5 pt-1">
        {/* The game's chat, as the player would read it later: dark, one line each, a soft shadow. */}
        <ol
          aria-label={t("afkPlus.intro.chatLabel")}
          className="flex flex-col gap-1 rounded-[var(--radius-panel)] bg-obsidian-950/70 px-3.5 py-3 font-display text-[13.5px] leading-snug [text-shadow:0_1px_0_rgb(0_0_0/0.55)]"
        >
          {CHAT.map((line, index) => (
            <motion.li
              key={line.key}
              initial={{ opacity: 0, transform: reduce ? "none" : "translateY(4px)" }}
              animate={{
                opacity: 1,
                transform: reduce ? "none" : "translateY(0px)",
                transition: { duration: 0.3, delay: 0.12 + index * 0.06, ease: easeOutExpo },
              }}
              className="text-bone-100"
            >
              {line.from === "bot" ? (
                <>
                  {/* A quiet speaker's name, as the other player's: the [AFK+] tag is the
                      player's choice and off by default, so other players would not see one. */}
                  <span className="font-semibold text-bone-100">
                    {t("afkPlus.messages.assistant")}:
                  </span>{" "}
                  <span className="text-bone-50">{t(`afkPlus.intro.chat.${line.key}`)}</span>
                </>
              ) : (
                <>
                  <span className="text-ash-300">{t("afkPlus.intro.chat.them")}:</span>{" "}
                  {t(`afkPlus.intro.chat.${line.key}`)}
                </>
              )}
            </motion.li>
          ))}
        </ol>
        <p className="text-[13px] text-bone-100">{t("afkPlus.intro.outcome")}</p>
        <p className="text-[12.5px] text-ash-300">{t("afkPlus.intro.promise")}</p>
        {/* Where it works, said where the player decides (the site's buy page says the same). */}
        <p className="text-[12.5px] text-ash-300">
          {t(
            plusOffer(overview)?.features.anyServer
              ? "afkPlus.intro.anyServer"
              : "afkPlus.intro.projectsOnly",
          )}
        </p>
        {signedOut ? (
          <p className="flex flex-wrap items-center gap-x-2 gap-y-1 text-[12.5px] text-ash-300">
            <span>{t("cloudAfk.intro.signIn")}</span>
            <button
              type="button"
              onClick={signIn}
              className="cursor-pointer rounded-sm font-medium text-bone-100 underline decoration-white/30 underline-offset-[3px] outline-none transition-[color,text-decoration-color] duration-150 hover:text-bone-50 hover:decoration-current focus-visible:ring-2 focus-visible:ring-gold-300"
            >
              {t("cloudAfk.intro.signInAction")}
            </button>
          </p>
        ) : null}
      </div>
    </Dialog>
  );
}
