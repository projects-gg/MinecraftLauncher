// INST-ADD: "Daha Fazla Seçenek" in "Yeni Kurulum" (the official launcher's "More options"), folded:
// the memory and JVM arguments the new instance starts with (Settings → Oyun's defaults until the
// player changes them here). Java and the window size are the instance's own settings: one line
// says where, and creating with the section open lands on that "Ayarlar" tab.
import { CaretDown } from "@phosphor-icons/react";
import { cn, easeOutExpo, Field, Input, Segmented, Slider, useMotionPrefs } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useRef } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { formatMemory } from "@/lib/format";
import { customMemoryStart, memoryCeiling } from "./create-more";

export function MoreOptions({
  open,
  onOpenChange,
  memoryMb,
  onMemoryChange,
  jvmArgs,
  onJvmArgsChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The new instance's maximum memory; `null` is "Otomatik". */
  memoryMb: number | null;
  onMemoryChange: (memoryMb: number | null) => void;
  jvmArgs: string;
  onJvmArgsChange: (jvmArgs: string) => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const id = useId();
  const { data: info } = useQuery({
    queryKey: ["app-info"],
    queryFn: () => api.appInfo(),
    enabled: open,
  });
  // Opened by the player: the fields come into view (on a small window they start below the fold).
  const content = useRef<HTMLDivElement>(null);
  const asked = useRef(false);
  useEffect(() => {
    if (!open || !asked.current) return;
    asked.current = false;
    const frame = requestAnimationFrame(() =>
      content.current?.scrollIntoView({ block: "nearest", behavior: reduce ? "auto" : "smooth" }),
    );
    return () => cancelAnimationFrame(frame);
  }, [open, reduce]);
  const total = info?.systemMemoryMb ?? 8192;
  const ceiling = memoryCeiling(total);
  // Content comes and goes by fading in place; the dialog's panel glides to its new size (morph).
  const appear = {
    initial: { opacity: 0, transform: reduce ? "translateY(0px)" : "translateY(4px)" },
    animate: {
      opacity: 1,
      transform: "translateY(0px)",
      transition: { duration: 0.26, ease: easeOutExpo },
    },
    exit: { opacity: 0, transition: { duration: 0.14, ease: easeOutExpo } },
  };
  return (
    <section className="grid gap-4">
      <h3 className="self-start">
        <button
          type="button"
          aria-expanded={open}
          aria-controls={open ? id : undefined}
          onClick={() => {
            asked.current = !open;
            onOpenChange(!open);
          }}
          className="-ml-1 flex cursor-pointer items-center gap-2 rounded-md px-1 py-0.5 text-[13px] font-semibold text-ash-300 transition-[color,transform,scale] duration-150 ease-out hover:text-bone-50 focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none active:scale-[0.97] aria-expanded:text-bone-50"
        >
          <CaretDown
            size={13}
            weight="bold"
            className={cn(
              "transition-transform duration-200 ease-out motion-reduce:transition-none",
              !open && "-rotate-90",
            )}
          />
          {t("instAdd.more.title")}
        </button>
      </h3>
      <AnimatePresence initial={false}>
        {open ? (
          <motion.div
            key="more"
            ref={content}
            id={id}
            // Scrolled into view with room for the dialog body's fading edge.
            className="grid scroll-mb-8 gap-4"
            {...appear}
          >
            <div className="grid grid-cols-2 items-start gap-4">
              <Field
                label={t("instance.settings.memory")}
                help={memoryMb === null ? t("settings.game.memoryAutoHelp") : undefined}
              >
                <div className="flex flex-col gap-3">
                  <Segmented
                    className="self-start"
                    value={memoryMb === null ? "auto" : "custom"}
                    onValueChange={(v) =>
                      onMemoryChange(v === "auto" ? null : customMemoryStart(total, memoryMb))
                    }
                    label={t("instance.settings.memory")}
                    options={[
                      { value: "auto", label: t("instance.settings.memoryAuto") },
                      { value: "custom", label: t("instance.settings.memoryCustom") },
                    ]}
                  />
                  <AnimatePresence initial={false}>
                    {memoryMb === null ? null : (
                      <motion.div key="slider" {...appear}>
                        <Slider
                          label={t("settings.game.maxMemory")}
                          value={Math.min(memoryMb, ceiling)}
                          onValueChange={onMemoryChange}
                          min={1024}
                          max={ceiling}
                          step={512}
                          format={formatMemory}
                        />
                      </motion.div>
                    )}
                  </AnimatePresence>
                </div>
              </Field>
              <Field
                label={t("instance.settings.jvmArgs")}
                help={t("graphics.jvmHelp")}
                htmlFor="new-instance-jvm-args"
              >
                <Input
                  id="new-instance-jvm-args"
                  value={jvmArgs}
                  onChange={(e) => onJvmArgsChange(e.target.value)}
                  spellCheck={false}
                  autoComplete="off"
                />
              </Field>
            </div>
            <p className="text-pretty text-[12.5px] text-ash-300">{t("instAdd.more.later")}</p>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </section>
  );
}
