// A first setup, one question a step (SETUP-UX-L, the site's setup-frame.tsx in the launcher's morph
// Dialog): on the left a rail of the questions, each answered one with its answer under it (a press
// goes back to it) and the current one on the rail's quiet plate; on the right the step: its
// question with its item, one line, its fields. Under them, in the dialog's bar, the count, "Daha
// sonra" on the first step and "Geri" after it, and the one gold action. The panel glides to each
// step's size (`morph`) while the new step comes in from the side it comes from (24 px, 280 ms; the
// old one leaves in 180 ms); an answer that lands on the rail rises 4 px into place and the plate
// glides to the next question (a spring, 480/38). Reduced motion and Performance mode: fades only.
// Closing (✕, Esc) is "Daha sonra": the choices wait.
import { Button, cn, Dialog, easeOutExpo, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { OneGold } from "../../settings/controls";
import { ItemSlot } from "./item-slot";

export interface FrameStep {
  id: string;
  question: string;
  item: string;
  /** The answer so far (on the rail once the step is behind). */
  answer?: string | null;
  /** Not asked (the money step while every order is a sale): on the rail with its answer only. */
  skipped?: boolean;
}

/** A step's way in and out: variants, so a step leaving takes the direction of the one coming. */
function stepVariants(still: boolean) {
  const shift = (n: number) => (still ? "none" : `translateX(${n}px)`);
  return {
    enter: (direction: 1 | -1) => ({ opacity: 0, transform: shift(direction * 24) }),
    center: {
      opacity: 1,
      transform: shift(0),
      transition: { duration: 0.28, ease: easeOutExpo },
    },
    exit: (direction: 1 | -1) => ({
      opacity: 0,
      transform: shift(direction * -24),
      transition: { duration: 0.18, ease: easeOutExpo },
    }),
  };
}

export function SetupDialog({
  open,
  title,
  where,
  steps,
  current,
  direction,
  onGo,
  onLater,
  railFoot,
  lede,
  children,
  onBack,
  primary,
  note,
  busy = false,
  failure,
}: {
  open: boolean;
  title: string;
  /** One line under the title (where it works). */
  where?: string | null;
  steps: FrameStep[];
  current: number;
  direction: 1 | -1;
  onGo: (index: number) => void;
  /** "Daha sonra", ✕ and Esc: the choices wait. */
  onLater: () => void;
  railFoot?: string;
  lede?: string | null;
  children: ReactNode;
  /** "Geri": none on the first step ("Daha sonra" there). */
  onBack?: () => void;
  primary: { label: string; onClick: () => void };
  /** Beside the primary (what happens after it). */
  note?: string | null;
  busy?: boolean;
  failure?: string | null;
}) {
  const { t } = useTranslation();
  const { layout } = useMotionPrefs();
  const asked = steps.filter((step) => !step.skipped);
  const step = steps[current];
  const position = step ? Math.max(0, asked.indexOf(step)) : 0;
  const footer = (
    <>
      <span className="mr-auto text-[12.5px] text-ash-400 tabular-nums">
        {t("setup.count", { step: position + 1, total: asked.length })}
      </span>
      {note ? <span className="text-[12px] text-pretty text-ash-400">{note}</span> : null}
      <Button variant="ghost" onClick={onBack ?? onLater} disabled={busy}>
        {onBack ? t("setup.back") : t("setup.later")}
      </Button>
      <Button variant="primary" onClick={primary.onClick} disabled={busy}>
        {primary.label}
      </Button>
    </>
  );
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && !busy && onLater()}
      title={title}
      description={where ?? undefined}
      closeLabel={t("setup.later")}
      width={780}
      morph
      dismissible={!busy}
      dismissOnOutsideClick={false}
      footer={footer}
    >
      {step ? (
        <div className="grid grid-cols-[196px_minmax(0,1fr)] gap-6 pt-1">
          <div className="flex min-w-0 flex-col">
            <ol aria-label={t("setup.stepsLabel")} className="flex flex-col gap-0.5">
              {steps.map((item, index) => {
                const now = index === current;
                const behind = index < current;
                const shown = behind || item.skipped ? item.answer : null;
                const content = (
                  <>
                    {now ? (
                      <motion.span
                        aria-hidden
                        layoutId={layout ? "setup-rail-plate" : undefined}
                        transition={{ type: "spring", stiffness: 480, damping: 38 }}
                        className="absolute inset-0 -z-10 rounded-md bg-white/[0.07]"
                      />
                    ) : null}
                    <ItemSlot item={item.item} size="sm" className="mt-px" />
                    <span className="flex min-w-0 flex-col gap-0.5">
                      <span
                        className={cn(
                          "text-[13px] leading-snug",
                          now
                            ? "font-semibold text-bone-50"
                            : behind
                              ? "font-medium text-bone-100"
                              : "font-medium text-ash-400",
                        )}
                      >
                        {item.question}
                      </span>
                      <AnimatePresence initial={false}>
                        {shown ? (
                          <motion.span
                            key={shown}
                            className="line-clamp-2 text-[12px] leading-snug text-ash-400"
                            initial={{ opacity: 0, transform: layout ? "translateY(4px)" : "none" }}
                            animate={{
                              opacity: 1,
                              transform: "translateY(0px)",
                              transition: { duration: 0.22, ease: easeOutExpo },
                            }}
                          >
                            {shown}
                          </motion.span>
                        ) : null}
                      </AnimatePresence>
                    </span>
                  </>
                );
                const shape =
                  "relative isolate flex w-full items-start gap-2.5 rounded-md px-2 py-2";
                return (
                  <li key={item.id}>
                    {behind && !item.skipped ? (
                      <button
                        type="button"
                        onClick={() => onGo(index)}
                        disabled={busy}
                        className={cn(
                          shape,
                          "cursor-pointer text-left outline-none transition-colors duration-150 hover:bg-white/[0.04] focus-visible:ring-2 focus-visible:ring-gold-300",
                        )}
                      >
                        {content}
                      </button>
                    ) : (
                      <div
                        aria-current={now ? "step" : undefined}
                        className={cn(shape, item.skipped && "opacity-70")}
                      >
                        {content}
                      </div>
                    )}
                  </li>
                );
              })}
            </ol>
            {railFoot ? (
              <p className="mt-auto px-2 pt-4 text-[12px] text-pretty text-ash-400">{railFoot}</p>
            ) : null}
          </div>

          <div className="min-w-0">
            <AnimatePresence initial={false} mode="popLayout" custom={direction}>
              <motion.div
                key={step.id}
                custom={direction}
                variants={stepVariants(!layout)}
                initial="enter"
                animate="center"
                exit="exit"
                className="min-w-0"
              >
                <header className="flex items-center gap-3">
                  <ItemSlot item={step.item} size="lg" />
                  <h3 className="min-w-0 font-display text-[20px] leading-tight font-semibold text-balance text-bone-50">
                    {step.question}
                  </h3>
                </header>
                {lede ? (
                  <p className="mt-3 max-w-[58ch] text-[13px] text-pretty text-ash-300">{lede}</p>
                ) : null}
                {/* The step's gold is "Devam"/"Başlat": its switches and links are bone (§2). */}
                <div className="mt-3">
                  <OneGold>{children}</OneGold>
                </div>
                {failure ? (
                  <p role="alert" className="mt-3 text-[12.5px] text-flare-300">
                    {failure}
                  </p>
                ) : null}
              </motion.div>
            </AnimatePresence>
          </div>
        </div>
      ) : null}
    </Dialog>
  );
}
