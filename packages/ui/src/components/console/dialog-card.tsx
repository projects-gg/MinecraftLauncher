// A server's dialog as a card in the console's command line place (DIALOG-CARD): its title and
// body in the game's looks, its inputs, its buttons. A command button waits for the player's word
// in ConsoleActionCard with the exact command its inputs build (the machine and the client refuse
// any other); a link too, https only. Close, custom and nested-dialog buttons go as the game's
// own button would. Nothing here runs on its own, and nothing of it reaches the AFK+ agent.
import { useId, useMemo, useState } from "react";
import { cn } from "../../lib/cn";
import { Button } from "../button";
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
  const field =
    "h-9 w-full rounded-md border border-white/10 bg-obsidian-900 px-3 font-mono text-[13px] text-bone-50 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-400/25 focus:outline-none";
  switch (input.kind) {
    case "bool":
      return (
        <Switch
          checked={value === true}
          onCheckedChange={(next) => onChange(next)}
          label={input.label}
        />
      );
    case "option":
      return (
        <label htmlFor={id} className="flex flex-col gap-1.5 text-[12.5px] text-ash-300">
          <span className={input.hideLabel ? "sr-only" : undefined}>{input.label}</span>
          <select
            id={id}
            className={field}
            value={String(value)}
            onChange={(event) => onChange(event.target.value)}
          >
            {input.options.map((option) => (
              <option key={option.id} value={option.id}>
                {option.label || option.id}
              </option>
            ))}
          </select>
        </label>
      );
    case "range": {
      const [lo, hi] =
        input.start <= input.end ? [input.start, input.end] : [input.end, input.start];
      return (
        <label htmlFor={id} className="flex flex-col gap-1.5 text-[12.5px] text-ash-300">
          <span className="flex justify-between">
            {input.label}
            <span className="font-mono text-bone-100 tabular-nums">{String(value)}</span>
          </span>
          <input
            id={id}
            type="range"
            min={lo}
            max={hi}
            step={input.step ?? "any"}
            value={Number(value)}
            onChange={(event) => onChange(snap(input, Number(event.target.value)))}
            className="accent-gold-400"
          />
        </label>
      );
    }
    default:
      return (
        <label htmlFor={id} className="flex flex-col gap-1.5 text-[12.5px] text-ash-300">
          <span className={input.hideLabel ? "sr-only" : undefined}>{input.label}</span>
          {input.multiline ? (
            <textarea
              id={id}
              rows={3}
              autoComplete="off"
              spellCheck={false}
              maxLength={input.max}
              className={cn(field, "h-auto py-2 leading-relaxed")}
              value={String(value)}
              onChange={(event) => onChange(event.target.value)}
            />
          ) : (
            <input
              id={id}
              type="text"
              autoComplete="off"
              spellCheck={false}
              maxLength={input.max}
              className={field}
              value={String(value)}
              onChange={(event) => onChange(event.target.value)}
            />
          )}
        </label>
      );
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
