// Where a Projects skin comes from (skin1.md §3.1, §13.2): "PNG yükle" (drop or pick a file, its
// arm model) or "Oyuncu adı" (a premium player's skin, looked up when typing pauses for 450 ms or
// on Enter). Both panes share one grid cell, so switching keeps the panel's height; what comes
// and goes inside a pane fades in place.
import { FolderOpen, UploadSimple, WarningCircle, X } from "@phosphor-icons/react";
import { Button, cn, IconButton, Input, Segmented, Skeleton } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { SkinFlat } from "@/features/skins/skin-flat";
import { projectsSkinApi } from "@/lib/api/projects-skin-api";
import type { ProjectsSkinChoice, SkinVariant } from "@/lib/api/projects-skin-types";
import { LOOKUP_DELAY_MS, lookupReady, premiumNameProblem } from "./rules";

export type SourceTab = "png" | "name";

/** A picked skin not saved yet. */
export interface Draft {
  choice: ProjectsSkinChoice;
  /** The arm model it is saved with (a premium skin keeps its own). */
  variant: SkinVariant;
  /** `png`: the arm model is a guess from the picture. */
  guessed: boolean;
}

const fade = {
  initial: { opacity: 0 },
  animate: { opacity: 1, transition: { duration: 0.15, delay: 0.06 } },
  exit: { opacity: 0, transition: { duration: 0.1 } },
};

/** A line under a control: an error (flare, announced) or a quiet note. */
function Line({ tone, children }: { tone: "error" | "warn" | "info"; children: string }) {
  return (
    <motion.p
      {...fade}
      role={tone === "error" ? "alert" : undefined}
      className={cn(
        "flex items-start gap-2 text-[12.5px] leading-snug",
        tone === "error" ? "text-flare-400" : tone === "warn" ? "text-gold-200" : "text-ash-400",
      )}
    >
      {tone === "info" ? null : (
        <WarningCircle size={15} weight="fill" className="mt-px shrink-0" aria-hidden />
      )}
      <span>{children}</span>
    </motion.p>
  );
}

function PngPane({
  draft,
  problem,
  reading,
  onChoose,
  onRemove,
  onVariant,
}: {
  draft: Draft | null;
  problem: string | null;
  reading: boolean;
  onChoose: () => void;
  onRemove: () => void;
  onVariant: (variant: SkinVariant) => void;
}) {
  const { t } = useTranslation();
  const file = draft?.choice.kind === "png" ? draft : null;
  const legacy = Boolean(file?.choice.legacy);
  return (
    <div className="flex flex-col gap-3">
      {file ? (
        <div className="flex items-center gap-4 rounded-[var(--radius-panel)] bg-obsidian-950/35 py-3 pr-3 pl-3 ring-1 ring-gold-400/30">
          <div className="flex h-[108px] w-[68px] shrink-0 items-center justify-center rounded-md bg-obsidian-950/60">
            <SkinFlat src={file.choice.picture} variant={file.variant} legacy={false} scale={3} />
          </div>
          <div className="min-w-0 flex-1">
            <p className="truncate text-[14px] font-medium text-bone-50" data-selectable>
              {file.choice.name}
            </p>
            <p className="mt-0.5 text-[12.5px] text-ash-400 tabular-nums">
              {t("skins.file.size", { width: 64, height: legacy ? 32 : 64 })}
            </p>
            <p className="mt-1.5 text-[12.5px] text-gold-200">{t("projectsSkin.file.pending")}</p>
          </div>
          <Button size="sm" variant="ghost" onClick={onChoose} disabled={reading}>
            {t("skins.file.another")}
          </Button>
          <IconButton
            size="sm"
            label={t("skins.file.remove")}
            icon={<X size={16} />}
            onClick={onRemove}
          />
        </div>
      ) : (
        <div className="flex items-center gap-4 rounded-[var(--radius-panel)] border border-dashed border-white/14 bg-obsidian-950/30 px-5 py-4">
          <UploadSimple size={26} className="shrink-0 text-ash-300" aria-hidden />
          <div className="min-w-0 flex-1">
            <p className="text-[14px] font-medium text-bone-50">{t("skins.file.title")}</p>
            <p className="mt-0.5 text-[12.5px] text-ash-400">{t("skins.file.body")}</p>
            <p className="mt-0.5 text-[12.5px] text-ash-400">{t("skins.file.help")}</p>
          </div>
          <Button
            size="sm"
            variant="secondary"
            icon={<FolderOpen size={15} />}
            onClick={onChoose}
            disabled={reading}
          >
            {reading ? t("skins.file.reading") : t("skins.file.choose")}
          </Button>
        </div>
      )}
      <AnimatePresence initial={false} mode="popLayout">
        {problem ? (
          <Line key="problem" tone="error">
            {problem}
          </Line>
        ) : file?.choice.blackParts ? (
          <Line key="black" tone="warn">
            {t("projectsSkin.file.black")}
          </Line>
        ) : null}
      </AnimatePresence>
      <AnimatePresence initial={false}>
        {file ? (
          <motion.div key="model" {...fade} className="flex items-center justify-between gap-6">
            <div className="min-w-0">
              <p className="text-[13.5px] font-medium text-bone-50">{t("skins.model.title")}</p>
              {legacy ? (
                <p className="mt-0.5 max-w-[46ch] text-[12.5px] text-ash-400">
                  {t("projectsSkin.file.legacy")}
                </p>
              ) : file.guessed ? (
                <p className="mt-0.5 max-w-[46ch] text-[12.5px] text-ash-400">
                  {t("skins.model.guessed")}
                </p>
              ) : null}
            </div>
            <Segmented<SkinVariant>
              value={legacy ? "classic" : file.variant}
              label={t("skins.model.title")}
              disabled={legacy}
              onValueChange={onVariant}
              options={[
                { value: "classic", label: t("skins.model.classic") },
                { value: "slim", label: t("skins.model.slim") },
              ]}
            />
          </motion.div>
        ) : null}
      </AnimatePresence>
    </div>
  );
}

type Lookup =
  | { status: "idle" }
  | { status: "looking" }
  | { status: "found"; choice: ProjectsSkinChoice }
  | { status: "noAccount" | "noSkin" | "unreachable" };

function NamePane({
  accountId,
  active,
  onFound,
}: {
  accountId: string;
  /** The pane is on screen (its field takes the focus when it comes). */
  active: boolean;
  /** A premium skin to preview, or null when the field no longer names one. */
  onFound: (choice: ProjectsSkinChoice | null) => void;
}) {
  const { t } = useTranslation();
  const ids = useId();
  const field = useRef<HTMLInputElement>(null);
  const [name, setName] = useState("");
  const [submitted, setSubmitted] = useState(false);
  const [lookup, setLookup] = useState<Lookup>({ status: "idle" });
  const seq = useRef(0);
  const found = useRef(onFound);
  found.current = onFound;

  const ask = async (raw: string) => {
    const mine = ++seq.current;
    setLookup({ status: "looking" });
    try {
      const answer = await projectsSkinApi.lookup(accountId, raw.trim());
      if (mine !== seq.current) return;
      if (answer.status === "found") {
        setLookup({ status: "found", choice: answer.choice });
        found.current(answer.choice);
      } else {
        setLookup({ status: answer.status });
        found.current(null);
      }
    } catch {
      if (mine !== seq.current) return;
      setLookup({ status: "unreachable" });
      found.current(null);
    }
  };

  // Typing pauses: ask once the name could exist; a change meanwhile starts the wait again. The
  // skin on preview stays until the next answer (one dip, not two); an emptied field drops it.
  // biome-ignore lint/correctness/useExhaustiveDependencies: asks for the name typed, nothing else
  useEffect(() => {
    if (!lookupReady(name)) {
      seq.current += 1;
      setLookup((current) =>
        current.status === "found" && name.trim() ? current : { status: "idle" },
      );
      if (!name.trim()) found.current(null);
      return;
    }
    const timer = window.setTimeout(() => void ask(name), LOOKUP_DELAY_MS);
    return () => window.clearTimeout(timer);
  }, [name]);

  useEffect(() => {
    if (active) requestAnimationFrame(() => field.current?.focus({ preventScroll: true }));
  }, [active]);

  const problem = premiumNameProblem(name, submitted);
  const resultLine =
    problem !== null ? (
      <Line key={`problem-${problem}`} tone="error">
        {t(`accounts.name.${problem}`)}
      </Line>
    ) : lookup.status === "looking" ? (
      <motion.div key="looking" {...fade} className="flex h-[18px] items-center" role="status">
        <span className="sr-only">{t("projectsSkin.name.looking")}</span>
        <Skeleton className="h-3 w-44" />
      </motion.div>
    ) : lookup.status === "found" ? (
      <Line key={`found-${lookup.choice.name}`} tone="info">
        {lookup.choice.own
          ? t("projectsSkin.name.own")
          : t("projectsSkin.name.found", { name: lookup.choice.name })}
      </Line>
    ) : lookup.status === "noAccount" || lookup.status === "unreachable" ? (
      <Line key={lookup.status} tone="error">
        {t(`projectsSkin.name.${lookup.status}`)}
      </Line>
    ) : lookup.status === "noSkin" ? (
      <Line key="noSkin" tone="info">
        {t("projectsSkin.name.noSkin")}
      </Line>
    ) : null;

  return (
    <div className="flex flex-col gap-2">
      <label htmlFor={`${ids}-name`} className="text-[13.5px] font-medium text-bone-50">
        {t("projectsSkin.name.label")}
      </label>
      <div className="relative max-w-[320px]">
        <Input
          ref={field}
          id={`${ids}-name`}
          value={name}
          maxLength={32}
          spellCheck={false}
          autoComplete="off"
          invalid={problem !== null}
          aria-describedby={`${ids}-result`}
          onChange={(event) => {
            setName(event.target.value);
            setSubmitted(false);
          }}
          onKeyDown={(event) => {
            if (event.key !== "Enter") return;
            event.preventDefault();
            setSubmitted(true);
            if (lookupReady(name)) void ask(name);
          }}
          className="pr-9"
        />
        <AnimatePresence initial={false}>
          {name ? (
            <motion.span
              key="clear"
              className="absolute top-[3px] right-[3px]"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1, transition: { duration: 0.12 } }}
              exit={{ opacity: 0, transition: { duration: 0.08 } }}
            >
              <IconButton
                size="sm"
                label={t("projectsSkin.name.clear")}
                icon={<X size={14} />}
                onClick={() => {
                  setName("");
                  setSubmitted(false);
                  found.current(null);
                  field.current?.focus();
                }}
              />
            </motion.span>
          ) : null}
        </AnimatePresence>
      </div>
      <div id={`${ids}-result`} className="min-h-[20px]" aria-live="polite">
        <AnimatePresence initial={false} mode="popLayout">
          {resultLine}
        </AnimatePresence>
      </div>
    </div>
  );
}

export function SourcePicker({
  accountId,
  mode,
  tab,
  onTab,
  draft,
  fileProblem,
  reading,
  onChooseFile,
  onRemoveFile,
  onVariant,
  onPremium,
  resetKey,
}: {
  accountId: string;
  /** `locked` while a change is on its way; `dim` while the player may not change skins. */
  mode: "open" | "locked" | "dim";
  tab: SourceTab;
  onTab: (tab: SourceTab) => void;
  draft: Draft | null;
  fileProblem: string | null;
  reading: boolean;
  onChooseFile: () => void;
  onRemoveFile: () => void;
  onVariant: (variant: SkinVariant) => void;
  onPremium: (choice: ProjectsSkinChoice | null) => void;
  /** Grows when the pick is dropped or saved: the name field starts empty again. */
  resetKey: number;
}) {
  const { t } = useTranslation();
  const [focusName, setFocusName] = useState(false);
  return (
    <fieldset
      disabled={mode !== "open"}
      className={cn(
        "m-0 min-w-0 border-0 p-0 transition-opacity duration-150",
        mode === "dim" && "opacity-55",
      )}
    >
      <legend className="sr-only">{t("projectsSkin.source.label")}</legend>
      <Segmented<SourceTab>
        value={tab}
        label={t("projectsSkin.source.label")}
        onValueChange={(next) => {
          setFocusName(next === "name");
          onTab(next);
        }}
        options={[
          { value: "png", label: t("projectsSkin.source.png") },
          { value: "name", label: t("projectsSkin.source.name") },
        ]}
      />
      <div className="mt-4 grid">
        <div
          className={cn(
            "[grid-area:1/1] transition-[opacity,visibility]",
            tab === "png"
              ? "visible opacity-100 delay-[60ms] duration-150"
              : "invisible opacity-0 duration-100",
          )}
          inert={tab !== "png"}
        >
          <PngPane
            draft={draft}
            problem={fileProblem}
            reading={reading}
            onChoose={onChooseFile}
            onRemove={onRemoveFile}
            onVariant={onVariant}
          />
        </div>
        <div
          className={cn(
            "[grid-area:1/1] transition-[opacity,visibility]",
            tab === "name"
              ? "visible opacity-100 delay-[60ms] duration-150"
              : "invisible opacity-0 duration-100",
          )}
          inert={tab !== "name"}
        >
          <NamePane
            key={resetKey}
            accountId={accountId}
            active={tab === "name" && focusName}
            onFound={onPremium}
          />
        </div>
      </div>
    </fieldset>
  );
}
