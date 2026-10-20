import { X } from "@phosphor-icons/react";
import { AnimatePresence, MotionConfig, motion, useReducedMotion } from "motion/react";
import { Dialog as RadixDialog } from "radix-ui";
import type { ReactNode } from "react";
import { EntranceGate, GatedMotion } from "../entrance";
import { focusPanel } from "../input-modality";
import { cn } from "../lib/cn";
import { dialogMotion, easeOutExpo, scrimMotion } from "../motion";
import { IconButton } from "./button";

/** How the panel follows a change of its content's size when `morph` is on. */
const morphTransition = { layout: { duration: 0.3, ease: easeOutExpo } };

/**
 * Centered modal. Title is required (accessibility); description optional. Content scrolls,
 * footer stays visible.
 *
 * `morph`: the panel glides to its new size when the content changes (multi-step dialogs)
 * instead of jumping; the motion is transform-based. It only measures when the panel itself
 * re-renders, so steps swap with `AnimatePresence mode="popLayout"` (the new step mounts in the
 * same render as the step change); with `mode="wait"` the new step arrives later, from inside
 * AnimatePresence, and its size change jumps. `dismissible={false}` hides the close button and
 * ignores Esc and outside clicks (while a step must not be interrupted).
 * `dismissOnOutsideClick={false}`: a click beside the panel does not close it (a form saved or
 * applied as a whole would lose its edits to a stray click); Esc and the close button still do.
 *
 * Opening waits for the frame that draws the panel (`EntranceGate`): the scrim and the panel
 * start their entrance on the next frame, so a heavy first render never eats its beginning.
 */
export function Dialog({
  open,
  onOpenChange,
  title,
  description,
  children,
  footer,
  width = 560,
  closeLabel,
  className,
  morph = false,
  dismissible = true,
  dismissOnOutsideClick = true,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
  width?: number;
  closeLabel: string;
  className?: string;
  morph?: boolean;
  dismissible?: boolean;
  dismissOnOutsideClick?: boolean;
}) {
  const block = (event: Event) => {
    if (!dismissible) event.preventDefault();
  };
  const blockOutside = (event: Event) => {
    // A toast's button (an error's "Ayrıntıları kopyala", "Geri al") is not a click away.
    const onToast = (event.target as Element | null)?.closest?.("[data-sonner-toaster]");
    if (!dismissible || !dismissOnOutsideClick || onToast) event.preventDefault();
  };
  const position = morph ? ("position" as const) : false;
  const reduce = useReducedMotion() ?? false;
  return (
    <RadixDialog.Root open={open} onOpenChange={onOpenChange}>
      <AnimatePresence>
        {open ? (
          // The entrance waits until the frame that draws the panel is done (entrance.tsx).
          <EntranceGate key="dialog">
            <RadixDialog.Portal forceMount>
              <RadixDialog.Overlay asChild forceMount>
                <GatedMotion.div
                  // Performance mode: a flat scrim, no blur (docs/design.md §5).
                  className="fixed inset-0 z-40 bg-obsidian-950/70 backdrop-blur-sm [html[data-perf=on]_&]:bg-obsidian-950/80 [html[data-perf=on]_&]:backdrop-blur-none"
                  {...scrimMotion(reduce)}
                />
              </RadixDialog.Overlay>
              <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center p-5">
                <RadixDialog.Content
                  asChild
                  forceMount
                  // Radix links the description itself; only a dialog without one opts out
                  // (an explicit `undefined` would also drop the link when there is one).
                  {...(description ? {} : { "aria-describedby": undefined })}
                  onEscapeKeyDown={block}
                  onPointerDownOutside={blockOutside}
                  onInteractOutside={blockOutside}
                  // The panel takes the focus, not the close button: focusing that button would pop
                  // its tooltip open on every dialog. Tab still reaches it first; an `autoFocus`
                  // field keeps the focus it took.
                  onOpenAutoFocus={(event) => {
                    event.preventDefault();
                    focusPanel(event.currentTarget as HTMLElement);
                  }}
                >
                  <GatedMotion.div
                    className="pointer-events-auto flex max-h-[88vh] w-full flex-col outline-none"
                    style={{ maxWidth: width }}
                    {...dialogMotion(reduce)}
                  >
                    <MotionConfig reducedMotion="user">
                      <motion.div
                        layout={morph}
                        transition={morphTransition}
                        style={{ borderRadius: 14 }}
                        className={cn(
                          "glass-strong relative flex min-h-0 flex-col overflow-hidden shadow-[0_40px_120px_rgb(34_6_6/0.6)]",
                          className,
                        )}
                      >
                        <motion.header
                          layout={position}
                          transition={morphTransition}
                          className="flex items-start justify-between gap-4 px-5 pt-4 pb-2.5"
                        >
                          <div className="min-w-0">
                            <RadixDialog.Title className="font-display text-section font-semibold text-bone-50">
                              {title}
                            </RadixDialog.Title>
                            {description ? (
                              <RadixDialog.Description className="mt-1 text-[13px] text-ash-300">
                                {description}
                              </RadixDialog.Description>
                            ) : null}
                          </div>
                          <AnimatePresence initial={false}>
                            {dismissible ? (
                              <motion.div
                                key="close"
                                initial={{ opacity: 0 }}
                                animate={{ opacity: 1, transition: { duration: 0.16 } }}
                                exit={{ opacity: 0, transition: { duration: 0.1 } }}
                              >
                                <RadixDialog.Close asChild>
                                  <IconButton label={closeLabel} icon={<X size={18} />} size="sm" />
                                </RadixDialog.Close>
                              </motion.div>
                            ) : null}
                          </AnimatePresence>
                        </motion.header>
                        <motion.div
                          layout={position}
                          layoutScroll={morph}
                          transition={morphTransition}
                          className="min-h-0 flex-1 overflow-y-auto px-5 pb-4"
                        >
                          {children}
                        </motion.div>
                        <AnimatePresence initial={false} mode="popLayout">
                          {footer ? (
                            <motion.footer
                              key="footer"
                              layout={position}
                              transition={morphTransition}
                              initial={{ opacity: 0 }}
                              animate={{ opacity: 1, transition: { duration: 0.2 } }}
                              exit={{ opacity: 0, transition: { duration: 0.12 } }}
                              className="flex w-full items-center justify-end gap-2 border-t border-white/7 bg-obsidian-950/30 px-5 py-3"
                            >
                              {footer}
                            </motion.footer>
                          ) : null}
                        </AnimatePresence>
                      </motion.div>
                    </MotionConfig>
                  </GatedMotion.div>
                </RadixDialog.Content>
              </div>
            </RadixDialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </RadixDialog.Root>
  );
}
