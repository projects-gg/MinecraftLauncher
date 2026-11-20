// The account settings' "Balık tutsun mu?" and "Maden kazsın mı?" (FISHMINE, README "Auto settings"):
// two questions after "Biri yazınca ne desin?", each only where Studio offers the mode for the account
// (`features.<mode>.mode` on), as on the site (the same questions and order, product.md "Ayarlar").
// A question leads with the mode's switch, its one sentence and the disclaimer; while no slot is free
// the switch waits and the sentence says why; on a limited server it shows disabled with the public
// reason and the limited servers page. The mode's choices fold in under the switch while it is on,
// the rod's or pickaxe's protection under "Gelişmiş". The whole setting is written at the first
// touch (the contract's defaults) and sent whole. The words are the site's.
import { ArrowSquareOut } from "@phosphor-icons/react";
import { cn, Input, Segmented, Switch } from "@pl/ui";
import { ItemSprite } from "@pl/ui/items";
import { type ReactNode, useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import type { CloudAutoFeature } from "@/bindings";
import { openWebLink } from "@/lib/links";
import { TEXT_LINK } from "@/lib/text-link";
import {
  ErrorText,
  Fold,
  inputClass,
  RangeRow,
  SettingRow,
  ToggleRow,
  WideRow,
} from "../../settings/controls";
import type { CloudEditData } from "../api";
import { PausedLine } from "../moderation-bits";
import { WalkButton } from "../movement/movement-panel";
import { PlusItemArt } from "../plus/item-icon";
import { itemLabel, usePlusItemData } from "../plus/items";
import { type Draft, type Issue, issueAt } from "../settings-model";
import { Advanced } from "../setup/question-list";
import { serverLabel } from "../status";
import { Disclaimer, limitedPage } from "./auto-panel";
import {
  type AutoMode,
  defaultFishing,
  defaultMining,
  type FishingSetting,
  fishingOf,
  type IdleMoves,
  MINING_BLOCKS,
  type MiningSetting,
  miningOf,
  ORES,
  orderedBlocks,
  STONES,
  slotKey,
  type ToolCare,
  type WhenFull,
} from "./model";

interface QuestionProps {
  draft: Draft;
  setDraft: (update: (draft: Draft) => Draft) => void;
  view: CloudEditData;
  issues: Issue[];
}

/** The account's Minecraft version for its item names and textures (none: the newest). */
const versionOf = (view: CloudEditData) =>
  view.account.version && view.account.version !== "auto" ? view.account.version : null;

/** A whole number as typed (NaN while it is not one, which the checks catch), its unit inside. */
function WholeField({
  label,
  value,
  onChange,
  unit,
  invalid,
  className,
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  unit?: string;
  invalid?: boolean;
  className?: string;
}) {
  const [text, setText] = useState(() => (Number.isFinite(value) ? String(value) : ""));
  // Follow outside changes (a reset) without fighting the typing.
  useEffect(() => {
    setText((current) =>
      Object.is(current.trim() === "" ? Number.NaN : Number(current), value)
        ? current
        : Number.isFinite(value)
          ? String(value)
          : current,
    );
  }, [value]);
  return (
    <div className={cn("relative w-[96px]", className)}>
      <input
        aria-label={label}
        inputMode="numeric"
        autoComplete="off"
        spellCheck={false}
        value={text}
        aria-invalid={invalid ? true : undefined}
        onChange={(event) => {
          setText(event.target.value);
          const typed = event.target.value.trim();
          onChange(/^\d{1,5}$/.test(typed) ? Number(typed) : Number.NaN);
        }}
        className={cn(
          inputClass,
          "h-[34px] px-3 tabular-nums",
          unit && "pr-9",
          invalid && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
        )}
      />
      {unit ? (
        <span className="pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-[12.5px] text-ash-400">
          {unit}
        </span>
      ) : null}
    </div>
  );
}

/** The mode's switch row: its name, its one sentence (or why it waits), the disclaimer. */
function Lead({
  mode,
  on,
  disabled,
  help,
  onChange,
}: {
  mode: AutoMode;
  on: boolean;
  disabled: boolean;
  help: ReactNode;
  onChange: (on: boolean) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  return (
    <div className="switch-row flex items-center justify-between gap-6 pt-1 pb-3">
      <div className="min-w-0">
        <label
          htmlFor={id}
          className={cn(
            // A row's label: the question above is the heading.
            "block text-[13.5px] font-medium text-bone-100",
            disabled ? "cursor-default opacity-55" : "cursor-pointer",
          )}
        >
          {t(`cloudAfkAuto.${mode}.title`)}
        </label>
        <p id={`${id}-help`} className="mt-0.5 max-w-[52ch] text-[12.5px] text-ash-400">
          {help}
        </p>
        <Disclaimer className="mt-1.5 max-w-[52ch]" />
      </div>
      <Switch
        id={id}
        describedBy={`${id}-help`}
        checked={on}
        onCheckedChange={onChange}
        label={t(`cloudAfkAuto.${mode}.title`)}
        hideLabel
        disabled={disabled}
      />
    </div>
  );
}

/** The rod's or pickaxe's protection: the durability it is put away at, and a spare. */
function CareRows({
  mode,
  care,
  error,
  onChange,
}: {
  mode: AutoMode;
  care: ToolCare;
  error: string | null;
  onChange: (care: ToolCare) => void;
}) {
  const { t } = useTranslation();
  return (
    <>
      <SettingRow
        lead
        title={t("cloudAfkAuto.keep")}
        help={t(`cloudAfkAuto.${mode}.keepHelp`)}
        error={error}
      >
        <WholeField
          label={t("cloudAfkAuto.keep")}
          value={care.keep}
          invalid={Boolean(error)}
          onChange={(keep) => onChange({ ...care, keep })}
        />
      </SettingRow>
      <ToggleRow
        title={t(`cloudAfkAuto.${mode}.switch`)}
        help={t(`cloudAfkAuto.${mode}.switchHelp`)}
        checked={care.switch}
        onCheckedChange={(on) => onChange({ ...care, switch: on })}
      />
    </>
  );
}

/**
 * What a full inventory does, and the command it sends; a command a moderation case paused says so
 * under it (the command rows' words), and a change has it read again.
 */
function FullRows({
  full,
  error,
  account,
  onChange: change,
}: {
  full: WhenFull;
  error: string | null;
  account: CloudEditData["account"];
  onChange: (full: WhenFull) => void;
}) {
  const onChange = (next: WhenFull) => change({ ...next, paused: null });
  const { t } = useTranslation();
  const id = useId();
  const action = full.do === "keep" || full.do === "command" ? full.do : "wait";
  return (
    <>
      <WideRow>
        <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2">
          <div className="min-w-0">
            <p className="text-[13.5px] font-medium text-bone-100">
              {t("cloudAfkAuto.full.title")}
            </p>
            <p id={`${id}-help`} className="mt-0.5 max-w-[52ch] text-[12.5px] text-ash-400">
              {t(`cloudAfkAuto.full.${action}Help`)}
            </p>
          </div>
          <Segmented
            value={action}
            onValueChange={(next) => onChange({ ...full, do: next })}
            label={t("cloudAfkAuto.full.title")}
            describedBy={`${id}-help`}
            options={[
              { value: "wait", label: t("cloudAfkAuto.full.wait") },
              { value: "keep", label: t("cloudAfkAuto.full.keep") },
              { value: "command", label: t("cloudAfkAuto.full.command") },
            ]}
          />
        </div>
      </WideRow>
      <Fold open={action === "command"}>
        <WideRow>
          <label htmlFor={`${id}-command`} className="text-[13px] font-medium text-bone-100">
            {t("cloudAfkAuto.full.commandLabel")}
          </label>
          <Input
            id={`${id}-command`}
            value={full.command ?? ""}
            placeholder={t("cloudAfkAuto.full.commandPlaceholder")}
            autoComplete="off"
            spellCheck={false}
            maxLength={256}
            invalid={Boolean(error)}
            onChange={(event) => onChange({ ...full, command: event.target.value })}
            className="mt-1.5"
          />
          {error ? (
            <div className="mt-1.5">
              <ErrorText>{error}</ErrorText>
            </div>
          ) : full.paused ? (
            <div className="mt-2 flex flex-col gap-1">
              <PausedLine category={full.paused.category} account={account} />
              <p className="text-[12.5px] text-ash-400">{t("cloudAfkAuto.full.pausedWait")}</p>
            </div>
          ) : null}
        </WideRow>
      </Fold>
    </>
  );
}

/** One block of the list: its texture, pressed in (gold edge) while chosen. */
function BlockPlate({
  id,
  name,
  chosen,
  onToggle,
}: {
  id: string;
  name: string;
  chosen: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      aria-label={name}
      aria-pressed={chosen}
      title={name}
      data-on={chosen ? "" : undefined}
      onClick={onToggle}
      className={cn(
        "group relative isolate grid size-[40px] cursor-pointer place-items-center select-none [--cut:6px]",
        "transition-[transform,scale] duration-150 ease-out active:scale-[0.96] focus-visible:outline-none",
      )}
    >
      <span
        aria-hidden
        className="chamfer-ring absolute -z-20 bg-gold-300 opacity-0 transition-opacity group-focus-visible:opacity-100"
      />
      <span
        aria-hidden
        className="chamfer absolute inset-0 -z-10 bg-white/10 transition-colors duration-150 group-hover:bg-white/20 group-data-[on]:bg-gold-500/70"
      />
      <span
        aria-hidden
        className="chamfer absolute inset-px -z-10 bg-obsidian-800 transition-colors duration-150 group-data-[on]:bg-[color-mix(in_oklab,var(--color-gold-500)_14%,var(--color-obsidian-800))]"
      />
      <span className="opacity-40 transition-opacity duration-150 group-hover:opacity-70 group-data-[on]:opacity-100">
        <ItemSprite id={id} size={16} />
      </span>
    </button>
  );
}

/** "Hangi Blokları Kırsın?": all of the list, or the player's choice of it in two groups. */
function BlocksRows({
  blocks,
  error,
  view,
  onChange,
}: {
  blocks: string[] | null;
  error: string | null;
  view: CloudEditData;
  onChange: (blocks: string[] | null) => void;
}) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language === "en" ? "en" : "tr";
  const id = useId();
  const version = versionOf(view);
  const { data } = usePlusItemData(version);
  const name = (block: string) => itemLabel(block, { data, custom: [], unnamed: "" }, lang);
  const some = blocks !== null;
  const chosen = blocks ?? [];
  const toggle = (block: string) =>
    onChange(
      orderedBlocks(
        chosen.includes(block) ? chosen.filter((b) => b !== block) : [...chosen, block],
      ),
    );
  const group = (title: string, list: readonly string[]) => (
    <div className="flex flex-col gap-1.5">
      <span className="text-[12px] font-medium text-ash-400">{title}</span>
      <div className="flex flex-wrap gap-1">
        {list.map((block) => (
          <BlockPlate
            key={block}
            id={block}
            name={name(block)}
            chosen={chosen.includes(block)}
            onToggle={() => toggle(block)}
          />
        ))}
      </div>
    </div>
  );
  return (
    <>
      <WideRow>
        <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2">
          <div className="min-w-0">
            <p className="text-[13.5px] font-medium text-bone-100">
              {t("cloudAfkAuto.blocks.title")}
            </p>
            <p id={`${id}-help`} className="mt-0.5 max-w-[52ch] text-[12.5px] text-ash-400">
              {some
                ? t("cloudAfkAuto.blocks.chosen", { count: chosen.length })
                : t("cloudAfkAuto.blocks.allHelp")}
            </p>
          </div>
          <Segmented
            value={some ? "some" : "all"}
            onValueChange={(next) =>
              // The choice starts from every block: taking away is quicker than adding.
              onChange(next === "all" ? null : [...MINING_BLOCKS])
            }
            label={t("cloudAfkAuto.blocks.title")}
            describedBy={`${id}-help`}
            options={[
              { value: "all", label: t("cloudAfkAuto.blocks.all") },
              { value: "some", label: t("cloudAfkAuto.blocks.some") },
            ]}
          />
        </div>
      </WideRow>
      <Fold open={some}>
        <WideRow className="flex flex-col gap-3">
          <PlusItemArt minecraft={version} server={serverLabel(view.account.server)}>
            {group(t("cloudAfkAuto.blocks.stones"), STONES)}
            {group(t("cloudAfkAuto.blocks.ores"), ORES)}
          </PlusItemArt>
          {error ? <ErrorText>{error}</ErrorText> : null}
        </WideRow>
      </Fold>
    </>
  );
}

/** "Arada Hareket Etsin": the idle moves while mining and their interval. */
function IdleRows({
  idle,
  error,
  onChange,
}: {
  idle: IdleMoves;
  error: string | null;
  onChange: (idle: IdleMoves) => void;
}) {
  const { t } = useTranslation();
  const any = idle.look || idle.sneak || idle.jump || idle.step;
  return (
    <>
      <WideRow>
        <p className="text-[13.5px] font-medium text-bone-100">{t("cloudAfkAuto.idle.title")}</p>
        <p className="mt-0.5 max-w-[52ch] text-[12.5px] text-ash-400">
          {t("cloudAfkAuto.idle.help")}
        </p>
        <div className="mt-2.5 flex flex-wrap gap-1.5">
          {(["look", "sneak", "jump", "step"] as const).map((move) => (
            <WalkButton
              key={move}
              label={t(`cloudAfkAuto.idle.${move}`)}
              ariaLabel={t(`cloudAfkAuto.idle.${move}`)}
              pressed={idle[move]}
              onClick={() => onChange({ ...idle, [move]: !idle[move] })}
            />
          ))}
        </div>
      </WideRow>
      <Fold open={any}>
        <RangeRow title={t("cloudAfkAuto.idle.every")} errors={[error]}>
          <WholeField
            label={t("cloudAfkAuto.idle.min")}
            value={idle.every.min}
            unit={t("cloudAfkAuto.idle.seconds")}
            invalid={Boolean(error)}
            onChange={(min) => onChange({ ...idle, every: { ...idle.every, min } })}
          />
          <WholeField
            label={t("cloudAfkAuto.idle.max")}
            value={idle.every.max}
            unit={t("cloudAfkAuto.idle.seconds")}
            invalid={Boolean(error)}
            onChange={(max) => onChange({ ...idle, every: { ...idle.every, max } })}
          />
        </RangeRow>
      </Fold>
    </>
  );
}

/** One mode's question: the switch, a refusal under it, its choices while on, "Gelişmiş". */
export function AutoQuestion({
  mode,
  draft,
  setDraft,
  view,
  issues,
}: QuestionProps & { mode: AutoMode }) {
  const { t, i18n } = useTranslation();
  const lang = i18n.language === "en" ? "en" : "tr";
  const feature = view.account.features?.[mode];
  if (feature?.mode !== "on") return null;
  const fishing = mode === "fishing" ? fishingOf(draft.config.fishing) : null;
  const mining = mode === "mining" ? miningOf(draft.config.mining) : null;
  const setting: FishingSetting | MiningSetting | null = fishing ?? mining;
  const on = setting?.enabled ?? feature.on ?? false;
  const limited = feature.limited ?? null;
  // A slot is missing only for turning it on: the stored "on" never waits.
  const waiting = !feature.on && feature.slot ? slotKey(feature.slot) : null;
  const refused = issueAt(issues, "general", null, mode);
  const refusal = refused
    ? refused.problem === "off" || refused.problem === "limited"
      ? { key: refused.problem }
      : slotKey(refused.problem)
    : null;
  const at = (part: string) => issueAt(issues, "general", null, `${mode}.${part}`)?.problem ?? null;
  const words = (part: string, problem: string | null): string | null => {
    if (!problem) return null;
    if (part === "rod" || part === "tool") return t("cloudAfkAuto.keepRange");
    if (part === "full") {
      if (problem === "empty" || problem === "secret") return t(`cloudAfkAuto.full.${problem}`);
      return t(`cloudAfk.settings.issues.${problem}`, {
        defaultValue: t("cloudAfkAuto.full.empty"),
      });
    }
    if (part === "blocks") return t("cloudAfkAuto.blocks.empty");
    return t(problem === "order" ? "cloudAfkAuto.idle.order" : "cloudAfkAuto.idle.range");
  };
  const write = (next: FishingSetting | MiningSetting) =>
    setDraft((d) => ({ ...d, config: { ...d.config, [mode]: next } }));
  const current = (): FishingSetting | MiningSetting =>
    setting ??
    (mode === "fishing"
      ? { ...defaultFishing(), enabled: on }
      : { ...defaultMining(), enabled: on });
  const careError = words(
    mode === "fishing" ? "rod" : "tool",
    at(mode === "fishing" ? "rod" : "tool"),
  );
  const help = limited ? (
    <>
      {limited[lang] || t(`cloudAfkAuto.${mode}.limited`)}{" "}
      <button
        type="button"
        className={cn(TEXT_LINK, "inline-flex items-center gap-1 font-medium")}
        onClick={() => openWebLink(limitedPage(lang))}
      >
        {t("cloudAfkAuto.limitedLink")}
        <ArrowSquareOut size={12} aria-hidden />
      </button>
    </>
  ) : waiting && !on ? (
    t(`cloudAfkAuto.${mode}.${waiting.key}`)
  ) : (
    t(`cloudAfkAuto.${mode}.line`)
  );
  const parts = mode === "fishing" ? ["rod", "full"] : ["blocks", "tool", "full", "idle"];
  return (
    <div className="flex flex-col">
      <Lead
        mode={mode}
        on={on}
        disabled={Boolean(limited) || (Boolean(waiting) && !on)}
        help={help}
        onChange={(enabled) => write({ ...current(), enabled })}
      />
      {/* A choice of the folded part still wrong (it goes with the setting): said here. */}
      {!on || limited
        ? parts
            .map((part) => words(part, at(part)))
            .filter((text): text is string => Boolean(text))
            .map((text) => (
              <div key={text} className="pb-2">
                <ErrorText>{text}</ErrorText>
              </div>
            ))
        : null}
      {refusal ? (
        <div className="pb-3">
          <ErrorText>
            {refusal.key === "limited" && limited?.[lang]
              ? limited[lang]
              : t(`cloudAfkAuto.${mode}.${refusal.key}`)}
          </ErrorText>
        </div>
      ) : null}
      <Fold open={on && !limited}>
        {fishing ? (
          <>
            <FullRows
              full={fishing.full}
              error={words("full", at("full"))}
              account={view.account}
              onChange={(full) => write({ ...fishing, full })}
            />
            <ToggleRow
              title={t("cloudAfkAuto.fishing.look")}
              help={t("cloudAfkAuto.fishing.lookHelp")}
              checked={fishing.look}
              onCheckedChange={(look) => write({ ...fishing, look })}
            />
            <Advanced summary={t("cloudAfkAuto.fishing.protection")} open={Boolean(careError)}>
              <CareRows
                mode="fishing"
                care={fishing.rod}
                error={careError}
                onChange={(rod) => write({ ...fishing, rod })}
              />
            </Advanced>
          </>
        ) : mining ? (
          <>
            <BlocksRows
              blocks={mining.blocks}
              error={words("blocks", at("blocks"))}
              view={view}
              onChange={(blocks) => write({ ...mining, blocks })}
            />
            <FullRows
              full={mining.full}
              error={words("full", at("full"))}
              account={view.account}
              onChange={(full) => write({ ...mining, full })}
            />
            <IdleRows
              idle={mining.idle}
              error={words("idle", at("idle"))}
              onChange={(idle) => write({ ...mining, idle })}
            />
            <Advanced summary={t("cloudAfkAuto.mining.protection")} open={Boolean(careError)}>
              <CareRows
                mode="mining"
                care={mining.tool}
                error={careError}
                onChange={(tool) => write({ ...mining, tool })}
              />
            </Advanced>
          </>
        ) : null}
      </Fold>
    </div>
  );
}

/** The mining answer's one block by its name in the game (its item data read only then). */
function OneBlock({
  view,
  block,
  text,
}: {
  view: CloudEditData;
  block: string;
  text: (name: string) => string;
}) {
  const { i18n } = useTranslation();
  const lang = i18n.language === "en" ? "en" : "tr";
  const { data } = usePlusItemData(versionOf(view));
  return <>{text(itemLabel(block, { data, custom: [], unnamed: "" }, lang))}</>;
}

/** A mode's answer in the question list, from the draft as it stands (the site's words). */
export function AutoAnswer({
  mode,
  draft,
  view,
}: {
  mode: AutoMode;
  draft: Draft;
  view: CloudEditData;
}) {
  const { t } = useTranslation();
  const feature: CloudAutoFeature | null | undefined = view.account.features?.[mode];
  const a = (key: string, values?: Record<string, string | number>) =>
    t(`cloudAfkAuto.answers.${key}`, values);
  if (feature?.limited) return <>{a("limited")}</>;
  const setting =
    mode === "fishing" ? fishingOf(draft.config.fishing) : miningOf(draft.config.mining);
  if (!setting?.enabled) return <>{a(mode === "fishing" ? "fishingOff" : "miningOff")}</>;
  const command = setting.full.command?.trim() ?? "";
  const tail =
    (setting.full.do === "keep"
      ? a("keep")
      : setting.full.do === "command" && command
        ? a("command", { command })
        : "") +
    (mode === "fishing"
      ? (setting as FishingSetting).look
        ? a("look")
        : ""
      : Object.entries((setting as MiningSetting).idle).some(
            ([key, on]) => key !== "every" && on === true,
          )
        ? a("idle")
        : "");
  if (mode === "fishing") return <>{a("fishingOn") + tail}</>;
  const blocks = (setting as MiningSetting).blocks;
  if (!blocks || blocks.length >= MINING_BLOCKS.length) return <>{a("miningAll") + tail}</>;
  const [only] = blocks;
  if (blocks.length === 1 && only) {
    return <OneBlock view={view} block={only} text={(block) => a("miningOne", { block }) + tail} />;
  }
  return <>{a("miningSome", { n: blocks.length }) + tail}</>;
}
