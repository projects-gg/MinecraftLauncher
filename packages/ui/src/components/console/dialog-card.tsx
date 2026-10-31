// A server's dialog as a card in the console's command line place (DIALOG-CARD; every dialog as a
// real form since CONSOLE-ANY): its title and body in the game's looks (an item part with the
// game's own tooltip), its inputs as the game draws them (a checkbox, radio buttons for up to five
// options and a list beyond, a slider in the game's steps and words, a text field of the game's
// lines), its buttons in the game's columns, widths and tooltips, a long list in pages. A command
// button waits for the player's word in ConsoleActionCard with the exact command its inputs build
// (the machine and the client refuse any other); a link too, https only. Close, custom and
// nested-dialog buttons go as the game's own button would. Nothing here runs on its own, and nothing
// of it reaches the AFK+ agent. The game's waiting screen and its pause and quick action menus are
// DialogWaiting and DialogMenus below.
import { CaretLeft, CaretRight, Check as CheckIcon } from "@phosphor-icons/react";
import { Checkbox, RadioGroup, Slider as RadixSlider } from "radix-ui";
import { type CSSProperties, useEffect, useId, useMemo, useState } from "react";
import { ItemSprite } from "../../items/item-sprite";
import { cn } from "../../lib/cn";
import { Button, IconButton } from "../button";
import { Field, Input, Select } from "../forms";
import { fill, type InventoryLang } from "../inventory/texts";
import { Tooltip } from "../tooltip";
import { ConsoleActionCard } from "./action-card";
import { ActIcon, ConsoleText, GameTipTarget, itemTipLines } from "./console-text";
import {
  type Dialog,
  type DialogButton,
  type DialogInput,
  type DialogMenu,
  type DialogMenuLabels,
  type DialogPart,
  type DialogRangeInput,
  type DialogTextInput,
  type DialogValues,
  dialogCommand,
  dialogInputs,
  fieldText,
  initialValues,
  sliderLabel,
  sliderSteps,
  sliderText,
  sliderValue,
  stepValue,
} from "./dialog";
import { type ConsoleClick, lineRuns } from "./looks";
import { CONSOLE_TEXTS } from "./texts";

export type DialogAct =
  | { i: number; inputs: Record<string, string>; cmd?: string }
  | { cancel: true };

export interface DialogCardProps {
  /** Checked: `safeDialog`. */
  dialog: Dialog;
  lang: InventoryLang;
  /** When it opened, as the console shows times. */
  clock: string;
  busy?: boolean;
  /** Why the last press did not go through (`dialogRefusal`, or the app's own error text). */
  error?: string | null;
  /** A button for the client (close, custom, a nested dialog, a confirmed command) or a close. */
  onAct: (act: DialogAct) => void;
  /** The card's local clicks: a confirmed link, a suggestion, a copy (the console's own handler). */
  onClick?: (click: ConsoleClick) => void;
  className?: string;
}

/** Buttons a page: a long list (the client sends up to 64) pages instead of growing the console. */
const PAGE = 8;

// A multiline text in the house field's look (forms.tsx has no text area).
const AREA =
  "w-full resize-none rounded-md border border-white/10 bg-obsidian-800/80 px-3 py-2 text-[13.5px] leading-relaxed text-bone-50 transition-[border-color,box-shadow] duration-150 hover:border-white/18 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-400/25 focus:outline-none";

/** A label in the game's looks (plain words where it has none). */
function Words({ text, st, lang }: { text: string; st?: unknown; lang: InventoryLang }) {
  return <ConsoleText text={text} st={st} lang={lang} />;
}

/** A text field as the game's: one line, or its lines (taller as the game sizes it). */
function TextField({
  input,
  value,
  onChange,
  lang,
}: {
  input: DialogTextInput;
  value: string;
  onChange: (value: string) => void;
  lang: InventoryLang;
}) {
  const id = useId();
  const label = input.hideLabel ? input.label : undefined;
  // The game sizes a multiline field by its lines or its height (9 of its pixels a line).
  const rows = Math.min(
    8,
    Math.max(2, input.lines ?? (input.height ? Math.round(input.height / 9) : 3)),
  );
  const field = input.multiline ? (
    <textarea
      id={id}
      rows={rows}
      autoComplete="off"
      spellCheck={false}
      maxLength={input.max}
      aria-label={label}
      className={AREA}
      value={value}
      onChange={(event) => onChange(fieldText(input, event.target.value))}
    />
  ) : (
    <Input
      id={id}
      type="text"
      autoComplete="off"
      spellCheck={false}
      maxLength={input.max}
      aria-label={label}
      value={value}
      onChange={(event) => onChange(fieldText(input, event.target.value))}
    />
  );
  return input.hideLabel ? (
    field
  ) : (
    <Field label={<Words text={input.label} st={input.labelSt} lang={lang} />} htmlFor={id}>
      {field}
    </Field>
  );
}

/** The game's checkbox: a small chamfered box, ticked bone (design.md §2), its label beside it. */
function CheckField({
  input,
  value,
  onChange,
  lang,
}: {
  input: Extract<DialogInput, { kind: "bool" }>;
  value: boolean;
  onChange: (value: boolean) => void;
  lang: InventoryLang;
}) {
  const id = useId();
  return (
    <div className="flex items-center gap-2.5">
      <Checkbox.Root
        id={id}
        checked={value}
        onCheckedChange={(next) => onChange(next === true)}
        aria-label={input.hideLabel ? input.label : undefined}
        className={cn(
          "group/check relative isolate inline-flex size-[18px] shrink-0 cursor-pointer items-center justify-center [--cut:4px]",
          "transition-transform duration-150 ease-[cubic-bezier(0.23,1,0.32,1)] active:scale-[0.9] focus-visible:outline-none",
          "[--check-edge:rgb(255_255_255/0.24)] [--check-on:var(--color-bone-100)] hover:[--check-edge:rgb(255_255_255/0.4)] hover:[--check-on:var(--color-bone-50)]",
        )}
      >
        <span
          aria-hidden
          className="chamfer-ring absolute -z-20 bg-gold-300 opacity-0 transition-opacity group-focus-visible/check:opacity-100"
        />
        <span
          aria-hidden
          className="chamfer absolute inset-0 -z-10 bg-(--check-edge) transition-colors duration-150 group-data-[state=checked]/check:bg-(--check-on)"
        />
        <span
          aria-hidden
          className="chamfer absolute inset-[1.5px] -z-10 bg-obsidian-900 transition-colors duration-150 [--cut:3px] group-data-[state=checked]/check:bg-(--check-on)"
        />
        <Checkbox.Indicator forceMount className="absolute inset-0 text-obsidian-900">
          <CheckIcon
            size={12}
            weight="bold"
            className="absolute inset-0 m-auto scale-[0.6] opacity-0 transition-[transform,scale,opacity] duration-150 ease-[cubic-bezier(0.23,1,0.32,1)] group-data-[state=checked]/check:scale-100 group-data-[state=checked]/check:opacity-100"
          />
        </Checkbox.Indicator>
      </Checkbox.Root>
      {input.hideLabel ? null : (
        <label htmlFor={id} className="cursor-pointer text-[13px] font-medium text-bone-100">
          <Words text={input.label} st={input.labelSt} lang={lang} />
        </label>
      )}
    </div>
  );
}

/** One of a server's options: up to five as radio buttons (the chosen one on the quiet plate), a list beyond. */
function ChoiceField({
  input,
  value,
  onChange,
  lang,
}: {
  input: Extract<DialogInput, { kind: "option" }>;
  value: string;
  onChange: (value: string) => void;
  lang: InventoryLang;
}) {
  const id = useId();
  if (input.options.length > 5) {
    const select = (
      <Select
        value={value}
        onValueChange={(next) => onChange(next)}
        options={input.options.map((option) => ({
          value: option.id,
          label: option.label || option.id,
        }))}
        label={input.label}
      />
    );
    return input.hideLabel ? (
      select
    ) : (
      <Field label={<Words text={input.label} st={input.labelSt} lang={lang} />}>{select}</Field>
    );
  }
  return (
    <div className="flex flex-col gap-1.5">
      {input.hideLabel ? null : (
        <span id={id} className="text-[13px] font-medium text-bone-100">
          <Words text={input.label} st={input.labelSt} lang={lang} />
        </span>
      )}
      <RadioGroup.Root
        value={value}
        onValueChange={onChange}
        aria-labelledby={input.hideLabel ? undefined : id}
        aria-label={input.hideLabel ? input.label : undefined}
        className="grid gap-1"
      >
        {input.options.map((option) => (
          <RadioGroup.Item
            key={option.id}
            value={option.id}
            className={cn(
              "group flex w-full cursor-pointer items-center gap-2.5 rounded-[var(--radius-panel)] px-2.5 py-1.5 text-left outline-none ring-1 ring-white/[0.07] transition-[background-color,box-shadow] duration-150",
              "hover:bg-white/[0.04] focus-visible:ring-2 focus-visible:ring-gold-300",
              "data-[state=checked]:bg-white/[0.06] data-[state=checked]:ring-white/25",
            )}
          >
            <span
              aria-hidden
              className="flex size-4 shrink-0 items-center justify-center rounded-full border border-white/25 transition-colors duration-150 group-data-[state=checked]:border-bone-100"
            >
              <RadioGroup.Indicator className="size-2 rounded-full bg-bone-50" />
            </span>
            <span className="min-w-0 text-[13px] text-bone-50">
              <Words text={option.label || option.id} st={option.labelSt} lang={lang} />
            </span>
          </RadioGroup.Item>
        ))}
      </RadioGroup.Root>
    </div>
  );
}

/** Places of a slider without steps: the game's slider moves by its pixels; a thousand is finer. */
const PLACES = 1000;

/**
 * The game's slider: its words in the server's format ("Miktar: 16"), its value as the game
 * computes it in floats, a step at a time from the initial value (or a thousandth of the way
 * without steps). Arrow keys move a step, Page keys ten, Home and End to the ends.
 */
function RangeField({
  input,
  value,
  onChange,
}: {
  input: DialogRangeInput;
  value: number;
  onChange: (value: number) => void;
}) {
  const id = useId();
  const stepped = input.step !== null;
  const { low, high } = useMemo(
    () => (stepped ? sliderSteps(input) : { low: 0, high: PLACES }),
    [input, stepped],
  );
  // Where the thumb is: the step of the value, or its nearest thousandth.
  const position = stepped
    ? Math.round((value - input.initial) / (input.step ?? 1))
    : Math.round(
        (input.start === input.end ? 0.5 : (value - input.start) / (input.end - input.start)) *
          PLACES,
      );
  const words = sliderLabel(input, value);
  // A slider that cannot move (a server's start and end alike) shows its words alone.
  if (high <= low) {
    return <p className="text-[13px] font-medium text-bone-100">{words}</p>;
  }
  return (
    <div className="flex flex-col gap-2">
      <span id={id} className="text-[13px] font-medium text-bone-100 tabular-nums">
        {input.hideLabel ? sliderText(value) : words}
      </span>
      <RadixSlider.Root
        value={[Math.min(high, Math.max(low, position))]}
        min={low}
        max={high}
        step={1}
        onValueChange={([next]) => {
          if (next === undefined) return;
          onChange(stepped ? stepValue(input, next) : sliderValue(next / PLACES, input));
        }}
        className="relative flex h-5 w-full cursor-pointer touch-none items-center select-none"
      >
        <RadixSlider.Track className="relative h-1.5 grow overflow-hidden rounded-full bg-white/10">
          <RadixSlider.Range className="absolute h-full bg-bone-100" />
        </RadixSlider.Track>
        <RadixSlider.Thumb
          aria-labelledby={id}
          aria-label={input.hideLabel ? input.label : undefined}
          aria-valuetext={words}
          className="block size-[18px] rounded-full border-2 border-bone-100 bg-obsidian-900 shadow-[0_2px_8px_rgb(34_6_6/0.6)] transition-[transform,scale,border-color] duration-200 hover:scale-110 focus-visible:ring-4 focus-visible:ring-gold-400/30 focus-visible:outline-none"
        />
      </RadixSlider.Root>
    </div>
  );
}

/** An input in the house controls, drawn as the game draws its kind. */
function InputField({
  input,
  value,
  onChange,
  lang,
}: {
  input: DialogInput;
  value: string | boolean | number;
  onChange: (value: string | boolean | number) => void;
  lang: InventoryLang;
}) {
  switch (input.kind) {
    case "bool":
      return <CheckField input={input} value={value === true} onChange={onChange} lang={lang} />;
    case "option":
      return <ChoiceField input={input} value={String(value)} onChange={onChange} lang={lang} />;
    case "range":
      return <RangeField input={input} value={Number(value)} onChange={onChange} />;
    default:
      return <TextField input={input} value={String(value)} onChange={onChange} lang={lang} />;
  }
}

/** A body part: a text, or an item on its tile with the game's tooltip and its own words beside it. */
function Part({ part, lang }: { part: DialogPart; lang: InventoryLang }) {
  const words = part.text ? (
    <p className="min-w-0 font-mono text-[13px] leading-relaxed whitespace-pre-wrap text-bone-100">
      <ConsoleText text={part.text} st={part.st} lang={lang} />
    </p>
  ) : null;
  const item = part.item;
  if (!item) return words;
  const lines = itemTipLines(item, lang);
  return (
    <div className="flex items-center gap-3">
      <GameTipTarget
        lines={part.noTip ? null : lines}
        label={lines[0]?.text ?? item.id}
        className="relative flex size-10 shrink-0 items-center justify-center rounded-md bg-black/30 ring-1 ring-white/10 outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
      >
        <ItemSprite
          id={item.id}
          cmd={item.cmd}
          model={item.model}
          potion={item.potion}
          color={item.color}
          enchanted={item.enchanted}
          skin={item.skin}
          size={32}
        />
        {!part.noDeco && item.count > 1 ? (
          <span
            aria-hidden
            className="absolute right-0.5 bottom-0 font-display text-[11px] font-semibold text-bone-50 tabular-nums [text-shadow:1px_1px_0_rgb(63_63_63)]"
          >
            {item.count}
          </span>
        ) : null}
      </GameTipTarget>
      {words}
    </div>
  );
}

export function DialogCard({
  dialog,
  lang,
  clock,
  busy,
  error,
  onAct,
  onClick,
  className,
}: DialogCardProps) {
  const t = CONSOLE_TEXTS[lang];
  const titleId = useId();
  const [values, setValues] = useState<DialogValues>(() => initialValues(dialog));
  const [pending, setPending] = useState<{ button: DialogButton; click: ConsoleClick } | null>(
    null,
  );
  const [page, setPage] = useState(0);
  const inputs = useMemo(() => dialogInputs(dialog, values), [dialog, values]);
  const pages = Math.max(1, Math.ceil(dialog.buttons.length / PAGE));
  const shown = dialog.buttons.slice(page * PAGE, page * PAGE + PAGE);
  // The game's columns, at most two on the console's width (a narrow console wraps them).
  const cols = Math.min(dialog.cols, 2);

  const press = (button: DialogButton) => {
    if (!inputs) return;
    switch (button.a) {
      case "run_command": {
        const cmd = dialogCommand(dialog, button, values);
        if (cmd) setPending({ button, click: { a: "run_command", v: cmd } });
        return;
      }
      case "open_url":
        if (button.v) setPending({ button, click: { a: "open_url", v: button.v } });
        return;
      case "suggest_command":
      case "copy_to_clipboard":
        if (button.v) onClick?.({ a: button.a, v: button.v });
        // The game does what follows a press too (closes, keeps or waits): the client is told.
        onAct({ i: button.i, inputs });
        return;
      default:
        onAct({ i: button.i, inputs });
    }
  };

  if (pending) {
    return (
      <ConsoleActionCard
        click={pending.click}
        lang={lang}
        clock={clock}
        source={t.dialogSource}
        busy={busy}
        className={className}
        onCancel={() => setPending(null)}
        onConfirm={() => {
          const { button, click } = pending;
          setPending(null);
          if (!inputs) return;
          if (click.a === "run_command") {
            onAct({ i: button.i, inputs, cmd: click.v });
          } else {
            onClick?.(click);
            onAct({ i: button.i, inputs });
          }
        }}
      />
    );
  }

  const closeLabel = dialog.exit ?? null;
  return (
    <section
      aria-labelledby={titleId}
      aria-roledescription={t.dialogLabel}
      className={cn(
        "flex flex-col gap-3 rounded-[10px] border border-white/[0.08] bg-obsidian-800/80 p-4 font-sans",
        className,
      )}
    >
      <h3 id={titleId} className="font-mono text-[14px] font-semibold text-bone-50">
        <ConsoleText text={dialog.title} st={dialog.titleSt} lang={lang} />
      </h3>
      {dialog.body.map((part, i) => (
        // biome-ignore lint/suspicious/noArrayIndexKey: a dialog's parts are its order
        <Part key={i} part={part} lang={lang} />
      ))}
      {dialog.inputs.length ? (
        <div className="flex flex-col gap-3">
          {dialog.inputs.map((input) => (
            <InputField
              key={input.key}
              input={input}
              value={values[input.key] ?? input.initial}
              onChange={(value) => setValues((old) => ({ ...old, [input.key]: value }))}
              lang={lang}
            />
          ))}
        </div>
      ) : null}
      <div
        className="grid gap-2"
        style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` } as CSSProperties}
      >
        {shown.map((button) => {
          const command = button.a === "run_command";
          const off = busy || !inputs || (command && !dialogCommand(dialog, button, values));
          // A command or a link ends in the console's line icon, in its label's last colour, as a
          // console part does: the player sees which buttons ask before they act.
          const glyph = command || button.a === "open_url";
          const runs = glyph ? lineRuns(button.label, button.labelSt) : null;
          const control = (
            <Button
              key={button.i}
              variant="secondary"
              size="md"
              disabled={off}
              onClick={() => press(button)}
              // The game's width (its pixels at its usual scale of two), never past the column.
              className="w-full justify-self-center"
              style={button.w ? { maxWidth: `${button.w * 2}px` } : undefined}
            >
              <span className="truncate">
                <ConsoleText text={button.label} st={button.labelSt} lang={lang} />
                {glyph ? (
                  <ActIcon a={button.a} color={runs ? runs[runs.length - 1]?.color : undefined} />
                ) : null}
              </span>
            </Button>
          );
          return button.tip ? (
            <Tooltip
              key={button.i}
              content={<ConsoleText text={button.tip.text} st={button.tip.st} lang={lang} />}
            >
              {control}
            </Tooltip>
          ) : (
            control
          );
        })}
      </div>
      {pages > 1 ? (
        <div className="flex items-center justify-center gap-2 text-[12.5px] text-ash-300">
          <IconButton
            size="sm"
            label={t.dialogPrev}
            icon={<CaretLeft size={14} weight="bold" />}
            disabled={page === 0}
            onClick={() => setPage((p) => Math.max(0, p - 1))}
          />
          <span className="tabular-nums" aria-live="polite">
            {fill(t.dialogPage, { page: page + 1, pages })}
          </span>
          <IconButton
            size="sm"
            label={t.dialogNext}
            icon={<CaretRight size={14} weight="bold" />}
            disabled={page >= pages - 1}
            onClick={() => setPage((p) => Math.min(pages - 1, p + 1))}
          />
        </div>
      ) : null}
      {dialog.cut ? <p className="text-[12.5px] text-ash-300">{t.dialogCut}</p> : null}
      {/* While the client joins, a command button is off: one plain line says why (a tooltip on a
          disabled button never shows on touch). */}
      {!dialog.play && dialog.buttons.some((button) => button.a === "run_command") ? (
        <p className="text-[12.5px] text-ash-300">{t.dialogPlayOnly}</p>
      ) : null}
      {error ? (
        <p role="alert" className="text-[12.5px] text-flare-400">
          {error}
        </p>
      ) : null}
      {/* The dialog's own exit button (client 4.2.5), else the window's close; one way out, not two. */}
      {dialog.cancel && (closeLabel || !dialog.buttons.some((button) => button.a === "close")) ? (
        <Button
          variant="ghost"
          size="sm"
          className="self-end"
          disabled={busy}
          onClick={() => onAct({ cancel: true })}
        >
          {closeLabel ? (
            <ConsoleText text={closeLabel.text} st={closeLabel.st} lang={lang} />
          ) : (
            t.dialogClose
          )}
        </Button>
      ) : null}
    </section>
  );
}

/**
 * The game's "waiting for response" screen after a press (client 4.2.5): its words and its back
 * button, which the game holds for five seconds, counting down. A new dialog takes its place.
 */
export function DialogWaiting({
  lang,
  busy,
  error,
  onBack,
  className,
}: {
  lang: InventoryLang;
  busy?: boolean;
  error?: string | null;
  /** Leaves it (the close of its revision; the game sends nothing). */
  onBack: () => void;
  className?: string;
}) {
  const t = CONSOLE_TEXTS[lang];
  const [left, setLeft] = useState(5);
  useEffect(() => {
    if (left <= 0) return;
    const timer = setTimeout(() => setLeft((n) => n - 1), 1000);
    return () => clearTimeout(timer);
  }, [left]);
  return (
    <section
      aria-label={t.dialogLabel}
      className={cn(
        "flex flex-col gap-3 rounded-[10px] border border-white/[0.08] bg-obsidian-800/80 p-4 font-sans",
        className,
      )}
    >
      <p role="status" className="text-[13.5px] font-medium text-bone-50">
        {t.dialogWaiting}
      </p>
      {error ? (
        <p role="alert" className="text-[12.5px] text-flare-400">
          {error}
        </p>
      ) : null}
      <Button
        variant="secondary"
        size="sm"
        className="self-end tabular-nums"
        disabled={left > 0 || busy}
        onClick={onBack}
      >
        {left > 0 ? fill(t.dialogBackIn, { n: left }) : t.dialogBack}
      </Button>
    </section>
  );
}

/**
 * What the server's pause screen and quick actions key open (client 4.2.5), as the game labels
 * them: a quiet row above the command line while no dialog is open.
 */
export function DialogMenus({
  menus,
  lang,
  busy,
  onOpen,
  className,
}: {
  menus: DialogMenuLabels;
  lang: InventoryLang;
  busy?: boolean;
  onOpen: (menu: DialogMenu) => void;
  className?: string;
}) {
  const t = CONSOLE_TEXTS[lang];
  const shown = (["pause", "quick"] as const).filter((menu) => menus[menu]);
  if (!shown.length) return null;
  return (
    <fieldset
      aria-label={t.dialogMenus}
      className={cn("m-0 flex min-w-0 flex-wrap gap-2 border-0 p-0", className)}
    >
      {shown.map((menu) => {
        const label = menus[menu];
        return label ? (
          <Button key={menu} variant="ghost" size="sm" disabled={busy} onClick={() => onOpen(menu)}>
            <ConsoleText text={label.text} st={label.st} lang={lang} />
          </Button>
        ) : null;
      })}
    </fieldset>
  );
}
