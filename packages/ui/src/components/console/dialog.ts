// A server's dialog (DIALOG-CARD, the game's 1.21.6 dialogs) as the console's card shows it: the
// machine's `dialog` checked again here (the view is the last guard; the rule list is looks.ts's),
// the inputs as the client takes them, and the command a run_command button would run, filled in
// the way the client fills it (DialogManager.ApplyTemplate), so the card shows exactly what the
// client is asked to run and the machine and the client refuse any other.
import { type ConsoleSpan, clean, lineRuns, MAX_VALUE, safeClick } from "./looks";

export const DIALOG = {
  title: 128,
  bodyParts: 8,
  bodyText: 512,
  bodyLines: 16,
  inputs: 8,
  options: 16,
  optionId: 64,
  label: 64,
  buttons: 12,
  textMax: 256,
  value: 256,
  bytes: 8 * 1024,
} as const;

export type DialogButtonAction =
  | "run_command"
  | "open_url"
  | "suggest_command"
  | "copy_to_clipboard"
  | "custom"
  | "dialog"
  | "close";

export interface DialogText {
  text: string;
  st?: readonly ConsoleSpan[] | null;
}

interface InputHead {
  key: string;
  label: string;
  labelSt?: readonly ConsoleSpan[] | null;
  /** The game shows no label for it. */
  hideLabel?: boolean;
}

export type DialogInput =
  | (InputHead & { kind: "text"; max: number; multiline: boolean; initial: string })
  | (InputHead & { kind: "bool"; initial: boolean; onTrue: string; onFalse: string })
  | (InputHead & {
      kind: "option";
      options: { id: string; label: string; labelSt?: readonly ConsoleSpan[] | null }[];
      initial: string;
    })
  | (InputHead & {
      kind: "range";
      start: number;
      end: number;
      step: number | null;
      initial: number;
    });

export interface DialogButton {
  i: number;
  label: string;
  labelSt?: readonly ConsoleSpan[] | null;
  a: DialogButtonAction;
  /** run_command: the template (`$(key)` filled from the inputs); the link, suggestion or text. */
  v?: string;
}

export interface Dialog {
  title: string;
  titleSt?: readonly ConsoleSpan[] | null;
  body: DialogText[];
  inputs: DialogInput[];
  buttons: DialogButton[];
  /** The card may close it (the game lets the player close it, and closing runs no command). */
  cancel: boolean;
  /** In play (a dialog while the client is still joining runs no command). */
  play: boolean;
  cols: number;
}

const KEY = /^[A-Za-z0-9_]{1,32}$/;
const ACTIONS: ReadonlySet<string> = new Set([
  "run_command",
  "open_url",
  "suggest_command",
  "copy_to_clipboard",
  "custom",
  "dialog",
  "close",
]);

const isObject = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** A text of at most `max` units and `lines` lines outside the rule list (`\n` only between lines). */
const word = (value: unknown, max: number, lines = 1): value is string =>
  typeof value === "string" &&
  value.length <= max &&
  value.split("\n").length <= lines &&
  clean(value, lines > 1);

/** Spans over `text` when they read (else none: the text shows in the card's own look). */
const spansOf = (text: string, st: unknown): ConsoleSpan[] | null =>
  st !== undefined && lineRuns(text, st) ? (st as ConsoleSpan[]) : null;

const finite = (value: unknown): value is number =>
  typeof value === "number" && Number.isFinite(value) && Math.abs(value) <= 1e9;

function inputOf(value: unknown): DialogInput | null {
  if (!isObject(value) || typeof value.key !== "string" || !KEY.test(value.key)) return null;
  if (!word(value.label, DIALOG.label)) return null;
  const head: InputHead = { key: value.key, label: value.label };
  const labelSt = spansOf(value.label, value.labelSt);
  if (labelSt) head.labelSt = labelSt;
  if (value.hideLabel === true) head.hideLabel = true;
  switch (value.kind) {
    case "text": {
      const max = value.max;
      if (!Number.isInteger(max) || (max as number) < 1 || (max as number) > DIALOG.textMax)
        return null;
      const multiline = value.multiline === true;
      const initial = value.initial ?? "";
      if (!word(initial, max as number, multiline ? DIALOG.value : 1)) return null;
      return { ...head, kind: "text", max: max as number, multiline, initial };
    }
    case "bool": {
      if (typeof value.initial !== "boolean") return null;
      if (!word(value.onTrue, DIALOG.value) || !word(value.onFalse, DIALOG.value)) return null;
      return {
        ...head,
        kind: "bool",
        initial: value.initial,
        onTrue: value.onTrue,
        onFalse: value.onFalse,
      };
    }
    case "option": {
      const list = value.options;
      if (!Array.isArray(list) || list.length < 1 || list.length > DIALOG.options) return null;
      const options: { id: string; label: string; labelSt?: readonly ConsoleSpan[] | null }[] = [];
      for (const option of list) {
        if (!isObject(option) || !word(option.id, DIALOG.optionId) || option.id === "") return null;
        if (!word(option.label, DIALOG.label) || options.some((o) => o.id === option.id))
          return null;
        const st = spansOf(option.label, option.labelSt);
        options.push(st ? { id: option.id, label: option.label, labelSt: st } : { id: option.id, label: option.label });
      }
      const initial = value.initial;
      if (typeof initial !== "string" || !options.some((o) => o.id === initial)) return null;
      return { ...head, kind: "option", options, initial };
    }
    case "range": {
      const { start, end, step, initial } = value;
      if (!finite(start) || !finite(end) || !finite(initial)) return null;
      if (step !== undefined && step !== null && (!finite(step) || step <= 0)) return null;
      const [lo, hi] = start <= end ? [start, end] : [end, start];
      if (initial < lo || initial > hi) return null;
      return {
        ...head,
        kind: "range",
        start,
        end,
        step: (step as number | null | undefined) ?? null,
        initial,
      };
    }
    default:
      return null;
  }
}

function buttonOf(value: unknown): DialogButton | null {
  if (!isObject(value) || !Number.isInteger(value.i) || (value.i as number) < 0) return null;
  if (!word(value.label, DIALOG.label) || typeof value.a !== "string" || !ACTIONS.has(value.a)) {
    return null;
  }
  const button: DialogButton = {
    i: value.i as number,
    label: value.label,
    a: value.a as DialogButtonAction,
  };
  const labelSt = spansOf(value.label, value.labelSt);
  if (labelSt) button.labelSt = labelSt;
  if (button.a === "run_command") {
    // A template: the click rules hold for it as written (a filled command is checked again).
    if (typeof value.v !== "string" || !safeClick({ a: "run_command", v: value.v })) return null;
    button.v = value.v;
  } else if (
    button.a === "open_url" ||
    button.a === "suggest_command" ||
    button.a === "copy_to_clipboard"
  ) {
    if (!safeClick({ a: button.a, v: value.v })) return null;
    button.v = value.v as string;
  } else if (value.v !== undefined) {
    return null;
  }
  return button;
}

/** The machine's dialog, or null when it is not the contract's (the console then shows none). */
export function safeDialog(value: unknown): Dialog | null {
  if (!isObject(value)) return null;
  try {
    if (JSON.stringify(value).length > DIALOG.bytes) return null;
  } catch {
    return null;
  }
  if (!word(value.title, DIALOG.title)) return null;
  const titleSt = spansOf(value.title, value.titleSt);
  const body: DialogText[] = [];
  if (!Array.isArray(value.body) || value.body.length > DIALOG.bodyParts) return null;
  for (const part of value.body) {
    if (!isObject(part) || !word(part.text, DIALOG.bodyText, DIALOG.bodyLines)) return null;
    if (part.text.length === 0) return null;
    body.push({ text: part.text, st: spansOf(part.text, part.st) });
  }
  const inputs: DialogInput[] = [];
  if (!Array.isArray(value.inputs) || value.inputs.length > DIALOG.inputs) return null;
  for (const raw of value.inputs) {
    const input = inputOf(raw);
    if (!input || inputs.some((other) => other.key === input.key)) return null;
    inputs.push(input);
  }
  const buttons: DialogButton[] = [];
  if (!Array.isArray(value.buttons) || value.buttons.length > DIALOG.buttons) return null;
  for (const raw of value.buttons) {
    const button = buttonOf(raw);
    if (!button || buttons.some((other) => other.i === button.i)) return null;
    buttons.push(button);
  }
  const cols = Number.isInteger(value.cols) ? Math.min(4, Math.max(1, value.cols as number)) : 1;
  return {
    title: value.title,
    titleSt,
    body,
    inputs,
    buttons,
    cancel: value.cancel === true,
    play: value.play !== false,
    cols,
  };
}

/** An input's value as the player left it: text, `true`/`false`, an option's id, a number. */
export type DialogValues = Record<string, string | boolean | number>;

export function initialValues(dialog: Dialog): DialogValues {
  const out: DialogValues = {};
  for (const input of dialog.inputs) out[input.key] = input.initial;
  return out;
}

/**
 * A number as the client writes it (DialogManager.NumberToString): a whole number as an integer,
 * else up to six decimals without float noise (the machine and the client refuse any other text).
 */
export function numberText(value: number): string {
  if (Number.isInteger(value)) return String(value);
  return String(Number.parseFloat(value.toFixed(6)));
}

/** A slider's value on its step from its start (the game's own snapping). */
export function snap(input: Extract<DialogInput, { kind: "range" }>, value: number): number {
  const [lo, hi] = input.start <= input.end ? [input.start, input.end] : [input.end, input.start];
  let v = Math.min(hi, Math.max(lo, value));
  if (input.step) v = input.start + Math.round((v - input.start) / input.step) * input.step;
  return Number.parseFloat(Math.min(hi, Math.max(lo, v)).toFixed(6));
}

/**
 * The value an input gives the client, or null when the client would refuse it
 * (DialogManager.NormalizeInputValue): a text within its length (a line break only in a multiline
 * one), `true` or `false`, an option's id, a number in range.
 */
export function inputText(input: DialogInput, value: unknown): string | null {
  switch (input.kind) {
    case "text":
      return typeof value === "string" &&
        value.length <= input.max &&
        clean(value, input.multiline)
        ? value
        : null;
    case "bool":
      return typeof value === "boolean" ? String(value) : null;
    case "option":
      return typeof value === "string" && input.options.some((o) => o.id === value) ? value : null;
    case "range": {
      if (typeof value !== "number" || !Number.isFinite(value)) return null;
      const [lo, hi] =
        input.start <= input.end ? [input.start, input.end] : [input.end, input.start];
      return value >= lo && value <= hi ? numberText(value) : null;
    }
  }
}

/** What an input puts in a command template (DialogManager.ToTemplateValue). */
function templateValue(input: DialogInput, text: string): string {
  if (input.kind === "bool") return text === "true" ? input.onTrue : input.onFalse;
  if (input.kind === "text") return text.split("\\").join("\\\\").split('"').join('\\"');
  return text;
}

/** Every input as the client takes it (key → text), or null when one would be refused. */
export function dialogInputs(dialog: Dialog, values: DialogValues): Record<string, string> | null {
  const out: Record<string, string> = {};
  for (const input of dialog.inputs) {
    const text = inputText(input, values[input.key] ?? input.initial);
    if (text === null) return null;
    out[input.key] = text;
  }
  return out;
}

/**
 * The command a run_command button runs with these inputs, filled in as the client fills it (each
 * input in order, every `$(key)` replaced), or null when it is no command the contract allows (or
 * the client is still joining).
 */
export function dialogCommand(
  dialog: Dialog,
  button: DialogButton,
  values: DialogValues,
): string | null {
  if (button.a !== "run_command" || !button.v || !dialog.play) return null;
  const inputs = dialogInputs(dialog, values);
  if (!inputs) return null;
  let command = button.v;
  for (const input of dialog.inputs) {
    // split/join, as the client's ordinal Replace: a `$` in a value is never a pattern.
    command = command.split(`$(${input.key})`).join(templateValue(input, inputs[input.key] ?? ""));
  }
  if (!command.startsWith("/")) command = `/${command}`;
  return command.length <= MAX_VALUE && safeClick({ a: "run_command", v: command })
    ? command
    : null;
}

/** A press of the card as the Worker takes it (`POST /accounts/:id/dialog`). */
export type DialogPress =
  | { rev: number; i: number; inputs: Record<string, string>; cmd?: string }
  | { rev: number; cancel: true };

/**
 * The press of `button` with these inputs, or null when the client would refuse it. A command
 * button's press carries the command the card showed (`cmd`), which the player confirmed.
 */
export function dialogPress(
  rev: number,
  dialog: Dialog,
  button: DialogButton,
  values: DialogValues,
): DialogPress | null {
  const inputs = dialogInputs(dialog, values);
  if (!inputs) return null;
  if (button.a !== "run_command") return { rev, i: button.i, inputs };
  const cmd = dialogCommand(dialog, button, values);
  return cmd === null ? null : { rev, i: button.i, inputs, cmd };
}
