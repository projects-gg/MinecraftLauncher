import { X } from "@phosphor-icons/react";
import { AnimatePresence, motion } from "motion/react";
import { Dialog as RadixDialog } from "radix-ui";
import type { ReactNode } from "react";
import { cn } from "../lib/cn";
import { easeOutExpo } from "../motion";
import { IconButton } from "./button";

/**
 * Centered modal. Title is required (accessibility); description optional. Content scrolls,
 * footer stays visible.
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
}) {
  return (
    <RadixDialog.Root open={open} onOpenChange={onOpenChange}>
      <AnimatePresence>
        {open ? (
          <RadixDialog.Portal forceMount>
            <RadixDialog.Overlay asChild forceMount>
              <motion.div
                className="fixed inset-0 z-40 bg-obsidian-950/70 backdrop-blur-sm"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0, transition: { duration: 0.16 } }}
              />
            </RadixDialog.Overlay>
            <div className="pointer-events-none fixed inset-0 z-50 flex items-center justify-center p-6">
              <RadixDialog.Content
                asChild
                forceMount
                aria-describedby={description ? undefined : undefined}
              >
                <motion.div
                  className={cn(
                    "glass-strong pointer-events-auto flex max-h-[86vh] w-full flex-col overflow-hidden rounded-[14px] shadow-[0_40px_120px_rgb(34_6_6/0.6)] outline-none",
                    className,
                  )}
                  style={{ maxWidth: width }}
                  initial={{ opacity: 0, y: 16, scale: 0.985 }}
                  animate={{ opacity: 1, y: 0, scale: 1 }}
                  exit={{ opacity: 0, y: 8, transition: { duration: 0.18 } }}
                  transition={{ duration: 0.36, ease: easeOutExpo }}
                >
                  <header className="flex items-start justify-between gap-4 px-6 pt-5 pb-3">
                    <div className="min-w-0">
                      <RadixDialog.Title className="font-display text-[20px] font-semibold text-bone-50">
                        {title}
                      </RadixDialog.Title>
                      {description ? (
                        <RadixDialog.Description className="mt-1 text-[13px] text-ash-400">
                          {description}
                        </RadixDialog.Description>
                      ) : null}
                    </div>
                    <RadixDialog.Close asChild>
                      <IconButton label={closeLabel} icon={<X size={18} />} size="sm" />
                    </RadixDialog.Close>
                  </header>
                  <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-5">{children}</div>
                  {footer ? (
                    <footer className="flex items-center justify-end gap-2.5 border-t border-white/7 bg-obsidian-950/30 px-6 py-3.5">
                      {footer}
                    </footer>
                  ) : null}
                </motion.div>
              </RadixDialog.Content>
            </div>
          </RadixDialog.Portal>
        ) : null}
      </AnimatePresence>
    </RadixDialog.Root>
  );
}
