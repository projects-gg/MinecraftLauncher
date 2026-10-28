// Instance settings → Java → "Özel": the Javas the launcher knows (its own runtimes and the
// computer's) to pick from, "Gözat" for any other, and one line on what the chosen Java means
// for this instance's game.
import { FolderOpen } from "@phosphor-icons/react";
import { Button, Chip, cn, easeOutExpo, Skeleton } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { AnimatePresence, motion } from "motion/react";
import { RadioGroup } from "radix-ui";
import { useTranslation } from "react-i18next";
import { api, type JavaCheck, type JavaRuntime } from "@/lib/api";
import { locationLabel, runtimeTitle, sameJava, shortPath } from "./texts";

/** What a Java path is and whether it suits the instance (shared by the picker and the form). */
export function useJavaCheck(instanceId: string, path: string, enabled: boolean) {
  const trimmed = path.trim();
  return useQuery({
    queryKey: ["java-check", instanceId, trimmed],
    queryFn: () => api.java.check(instanceId, trimmed),
    enabled: enabled && trimmed.length > 0,
    staleTime: 60_000,
    retry: false,
  });
}

/** The sentence under the list: what is wrong with the choice, or nothing when it fits. */
export function javaProblemText(
  t: TFunction,
  path: string,
  check: JavaCheck | undefined,
): string | null {
  if (!path.trim()) return t("instance.settings.javaChoose");
  if (!check) return null;
  if (check.problem === "missing") return t("instance.settings.javaMissing");
  if (check.problem === "broken") return t("instance.settings.javaBroken");
  if (check.fit === "tooOld" && check.need) {
    return check.forMods
      ? t("instance.settings.javaModsNeed", { need: check.need })
      : t("instance.settings.javaNeed", { need: check.need });
  }
  if (check.fit === "tooNew" && check.need) {
    return t("instance.settings.javaMax", { need: check.need });
  }
  return null;
}

function Row({
  value,
  title,
  vendor,
  location,
  bits32,
  problem,
}: {
  value: string;
  title: string;
  vendor?: string;
  location: string;
  bits32?: boolean;
  problem?: boolean;
}) {
  const { t } = useTranslation();
  return (
    <RadioGroup.Item
      value={value}
      className={cn(
        "group relative flex w-full cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] px-3 py-2 text-left outline-none ring-1 ring-white/[0.07] transition-[background-color,box-shadow] duration-150",
        "hover:bg-white/[0.04] focus-visible:ring-2 focus-visible:ring-gold-300",
        // The chosen row a quiet plate, the gold only on its dot (design.md §6: one gold).
        "data-[state=checked]:bg-white/[0.06] data-[state=checked]:ring-white/25",
      )}
    >
      <span
        aria-hidden
        className="flex size-4 shrink-0 items-center justify-center rounded-full border border-white/25 transition-colors duration-150 group-data-[state=checked]:border-gold-400"
      >
        <RadioGroup.Indicator className="size-2 rounded-full bg-gold-400" />
      </span>
      <span className="min-w-0 flex-1">
        <span className="flex min-w-0 items-baseline gap-2">
          <span
            className={cn(
              "shrink-0 text-[13.5px] font-semibold",
              problem ? "text-flare-300" : "text-bone-50",
            )}
          >
            {title}
          </span>
          {vendor ? <span className="truncate text-[12.5px] text-ash-300">{vendor}</span> : null}
        </span>
        <span className="block truncate text-[12.5px] text-ash-400">{location}</span>
      </span>
      {bits32 ? (
        <Chip tone="flare" className="shrink-0">
          {t("java.bits32")}
        </Chip>
      ) : null}
    </RadioGroup.Item>
  );
}

/** A picked Java's title: "Java 21", or what is wrong with it. */
function checkTitle(t: TFunction, check: JavaCheck | undefined): string {
  if (!check) return t("instance.settings.javaChecking");
  if (check.problem === "missing") return t("java.missing");
  if (check.problem === "broken" || check.major === undefined) return t("java.broken");
  return t("java.title", { major: check.major });
}

/** The folder of a picked program (`…\jdk-21\bin\javaw.exe` → `…\jdk-21`), as written. */
function folderOf(path: string): string {
  return path.replace(/[\\/]bin[\\/]javaw?(\.exe)?$/i, "").replace(/[\\/]bin[\\/]?$/i, "");
}

function runtimeRow(t: TFunction, runtime: JavaRuntime) {
  return (
    <Row
      key={runtime.id}
      value={runtime.executable}
      title={runtimeTitle(t, runtime)}
      vendor={runtime.vendor}
      location={locationLabel(t, runtime)}
      bits32={runtime.bits32}
    />
  );
}

export function JavaPicker({
  instanceId,
  value,
  onChange,
}: {
  instanceId: string;
  /** The Java path of the form (empty: none chosen yet). */
  value: string;
  onChange: (path: string) => void;
}) {
  const { t } = useTranslation();
  const overview = useQuery({
    queryKey: ["java-overview"],
    queryFn: () => api.java.overview(),
    staleTime: 30_000,
  });
  const check = useJavaCheck(instanceId, value, true);
  // Launcher runtimes first, then the computer's (the overview's order); broken custom paths of
  // other instances are no choice.
  const listed = (overview.data?.runtimes ?? []).filter((r) => r.kind !== "custom");
  const chosen = listed.find((r) => sameJava(r.executable, value));
  const own = value.trim() && !chosen ? value.trim() : null;
  const problem = javaProblemText(t, value, check.data);

  const browse = async () => {
    const path = await api.java.pickExecutable();
    if (path) onChange(path);
  };

  return (
    <div className="grid gap-2">
      {overview.isPending ? (
        <div className="grid gap-1.5" aria-busy>
          <Skeleton className="h-[50px] w-full rounded-[var(--radius-panel)]" />
          <Skeleton className="h-[50px] w-full rounded-[var(--radius-panel)]" />
        </div>
      ) : (
        <RadioGroup.Root
          value={chosen?.executable ?? own ?? ""}
          onValueChange={onChange}
          aria-label={t("instance.settings.javaCustom")}
          className="grid gap-1.5"
        >
          {listed.map((runtime) => runtimeRow(t, runtime))}
          {own ? (
            <Row
              value={own}
              title={checkTitle(t, check.data)}
              vendor={check.data?.vendor}
              location={shortPath(folderOf(own))}
              bits32={check.data?.bits32}
              problem={Boolean(check.data?.problem)}
            />
          ) : null}
        </RadioGroup.Root>
      )}
      <div className="flex min-h-[32px] flex-wrap items-center gap-x-3 gap-y-1">
        <Button
          variant="ghost"
          size="sm"
          icon={<FolderOpen size={15} />}
          className="-ml-2.5"
          onClick={() => void browse()}
        >
          {t("instance.settings.javaBrowse")}
        </Button>
        {!overview.isPending && listed.length === 0 && !own ? (
          <span className="text-[12.5px] text-ash-400">{t("java.emptySystem")}</span>
        ) : null}
      </div>
      {/* One reserved line: the warning cross-fades in place and never moves what follows. */}
      <div className="relative min-h-[18px]" aria-live="polite">
        <AnimatePresence initial={false} mode="popLayout">
          {problem ? (
            <motion.p
              key={problem}
              role="alert"
              className="text-[12.5px] text-flare-400"
              initial={{ opacity: 0 }}
              animate={{
                opacity: 1,
                transition: { duration: 0.2, ease: easeOutExpo, delay: 0.05 },
              }}
              exit={{ opacity: 0, transition: { duration: 0.1 } }}
            >
              {problem}
            </motion.p>
          ) : null}
        </AnimatePresence>
      </div>
    </div>
  );
}
