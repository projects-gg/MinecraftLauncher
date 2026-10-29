// A server's dialog as a card in the console's command line place (DIALOG-CARD): its title and
// body in the game's looks, its inputs, its buttons. A command button waits for the player's word
// in ConsoleActionCard with the exact command its inputs build (the machine and the client refuse
// any other); a link too, https only. Close, custom and nested-dialog buttons go as the game's
// own button would. Nothing here runs on its own, and nothing of it reaches the AFK+ agent.
import { useId, useMemo, useState } from "react";
import { cn } from "../../lib/cn";
import { Button } from "../button";
import { Field, Input, Select, Slider } from "../forms";
import type { InventoryLang } from "../inventory/texts";
import { Switch } from "../switch";
import { ConsoleActionCard } from "./action-card";
import { ConsoleText } from "./console-text";
import {
  type Dialog,
  type DialogButton,
  type DialogInput,
  type DialogValues,
  dialogCommand,
  dialogInputs,
  initialValues,
  numberText,
  snap,
} from "./dialog";
import type { ConsoleClick } from "./looks";
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
  /** A button for the client (close, custom, a nested dialog, a confirmed command) or a close. */
  onAct: (act: DialogAct) => void;
  /** The card's local clicks: a confirmed link, a suggestion, a copy (the console's own handler). */
  onClick?: (click: ConsoleClick) => void;
  className?: string;
}

// A multiline text in the house field's look (forms.tsx has no text area).
const AREA =
  "min-h-[78px] w-full rounded-md border border-white/10 bg-obsidian-800/80 px-3 py-2 text-[13.5px] leading-relaxed text-bone-50 transition-[border-color,box-shadow] duration-150 hover:border-white/18 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-400/25 focus:outline-none";

/** An input in the house controls (Field, Input, Select, Slider, Switch: bone, design.md §2). */
function InputField({
  input,
  value,
  onChange,
}: {
  input: DialogInput;
  value: string | boolean | number;
  onChange: (value: string | boolean | number) => void;
}) {
  const id = useId();
  switch (input.kind) {
    case "bool":
      return (
        <Switch
          checked={value === true}
          onCheckedChange={(next) => onChange(next)}
          label={input.label}
        />
      );
    case "option": {
      const select = (
        <Select
          value={String(value)}
          onValueChange={(next) => onChange(next)}
          options={input.options.map((option) => ({
            value: option.id,
            label: option.label || option.id,
          }))}
          label={input.label}
        />
      );
      return input.hideLabel ? select : <Field label={input.label}>{select}</Field>;
    }
    case "range": {
      const [lo, hi] =
        input.start <= input.end ? [input.start, input.end] : [input.end, input.start];
      // A slider that cannot move (a server's start and end alike) shows its one value.
      if (hi <= lo) {
        return (
          <p className="flex items-baseline justify-between text-[13px] font-medium text-bone-100">
            {input.label}
            <span className="font-display text-[15px] font-semibold text-bone-50 tabular-nums">
              {numberText(lo)}
            </span>
          </p>
        );
      }
      return (
        <Slider
          value={Number(value)}
          onValueChange={(next) => onChange(snap(input, next))}
          min={lo}
          max={hi}
          step={input.step ?? (hi - lo) / 100}
          label={input.label}
          format={numberText}
        />
      );
    }
    default: {
      const label = input.hideLabel ? input.label : undefined;
      const field = input.multiline ? (
        <textarea
          id={id}
          rows={3}
          autoComplete="off"
          spellCheck={false}
          maxLength={input.max}
          aria-label={label}
          className={AREA}
          value={String(value)}
          onChange={(event) => onChange(event.target.value)}
        />
      ) : (
        <Input
          id={id}
          type="text"
          autoComplete="off"
          spellCheck={false}
          maxLength={input.max}
          aria-label={label}
          value={String(value)}
          onChange={(event) => onChange(event.target.value)}
        />
      );
      return input.hideLabel ? (
        field
      ) : (
        <Field label={input.label} htmlFor={id}>
          {field}
        </Field>
      );
    }
  }
}

export function DialogCard({
  dialog,
  lang,
  clock,
  busy,
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
  const inputs = useMemo(() => dialogInputs(dialog, values), [dialog, values]);

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
          if (click.a === "run_command" && inputs) onAct({ i: button.i, inputs, cmd: click.v });
          else onClick?.(click);
        }}
      />
    );
  }

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
        <p
          // biome-ignore lint/suspicious/noArrayIndexKey: a dialog's parts are its order
          key={i}
          className="font-mono text-[13px] leading-relaxed whitespace-pre-wrap text-bone-100"
        >
          <ConsoleText text={part.text} st={part.st} lang={lang} />
        </p>
      ))}
      {dialog.inputs.length ? (
        <div className="flex flex-col gap-3">
          {dialog.inputs.map((input) => (
            <InputField
              key={input.key}
              input={input}
              value={values[input.key] ?? input.initial}
              onChange={(value) => setValues((old) => ({ ...old, [input.key]: value }))}
            />
          ))}
        </div>
      ) : null}
      <div
        className="grid gap-2"
        style={{ gridTemplateColumns: `repeat(${Math.min(dialog.cols, 2)}, minmax(0, 1fr))` }}
      >
        {dialog.buttons.map((button) => {
          const command = button.a === "run_command";
          const off = busy || !inputs || (command && !dialogCommand(dialog, button, values));
          return (
            <Button
              key={button.i}
              variant="secondary"
              size="md"
              disabled={off}
              onClick={() => press(button)}
              title={command && !dialog.play ? t.dialogPlayOnly : undefined}
            >
              <ConsoleText text={button.label} st={button.labelSt} lang={lang} />
            </Button>
          );
        })}
      </div>
      {dialog.cancel ? (
        <Button
          variant="ghost"
          size="sm"
          className="self-end"
          onClick={() => onAct({ cancel: true })}
        >
          {t.dialogClose}
        </Button>
      ) : null}
    </section>
  );
}
