// A server's dialog (DIALOG-CARD, the game's 1.21.6 dialogs) as the console's card shows it: the
// machine's `dialog` checked again here (the view is the last guard; the rule list is looks.ts's),
// the inputs as the client takes them, and the command a run_command button would run, filled in
// the way the client fills it, so the card shows exactly what the client is asked to run and the
// machine and the client refuse any other. A client 4.2.5 dialog (CONSOLE-ANY) fills in and writes
// its values as Minecraft's own client does (InputControlHandlers, StringTag.escapeWithoutQuotes,
// StringTemplate, NumberRangeInput.RangeInfo, Java's Float.toString); a 4.2.4 one as 4.2.4 did.

import { safeInvItem } from "../inventory/safe-item";
import type { InventoryLang } from "../inventory/texts";
import type { InvItem } from "../inventory/types";
import { type ConsoleSpan, clean, lineRuns, MAX_VALUE, safeClick } from "./looks";
import { CONSOLE_TEXTS } from "./texts";

export const DIALOG = {
  title: 128,
  /** Client 4.2.5: a longer body or button list is cut by the client (`cut`). */
  bodyParts: 16,
  bodyText: 512,
  bodyLines: 16,
  inputs: 16,
  options: 16,
  optionId: 64,
  label: 64,
  tooltip: 256,
  tooltipLines: 8,
  buttons: 64,
  textMax: 256,
  value: 256,
  format: 64,
  bytes: 16 * 1024,
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

/** A body part: a text, or (client 4.2.5) an item with its own description as the text. */
export interface DialogPart extends DialogText {
  item?: InvItem;
  /** The game draws no tooltip for the item. */
  noTip?: boolean;
  /** The game draws the item without its count and wear bar. */
  noDeco?: boolean;
}

interface InputHead {
  key: string;
  label: string;
  labelSt?: readonly ConsoleSpan[] | null;
  /** The game shows no label for it. */
  hideLabel?: boolean;
}

export type DialogTextInput = InputHead & {
  kind: "text";
  max: number;
  multiline: boolean;
  /** A multiline field's most lines (client 4.2.5). */
  lines: number | null;
  /** A multiline field's height in the game's pixels (client 4.2.5). */
  height: number | null;
  initial: string;
};

export type DialogRangeInput = InputHead & {
  kind: "range";
  start: number;
  end: number;
  step: number | null;
  /** The value before the slider moves (client 4.2.5: the game's own; with a step, where its steps count from). */
  initial: number;
  /** The label's format in the client's language, two `%s` or `%1$s` and `%2$s` (client 4.2.5). */
  fmt: string | null;
};

export type DialogInput =
  | DialogTextInput
  | (InputHead & { kind: "bool"; initial: boolean; onTrue: string; onFalse: string })
  | (InputHead & {
      kind: "option";
      options: { id: string; label: string; labelSt?: readonly ConsoleSpan[] | null }[];
      initial: string;
    })
  | DialogRangeInput;

export interface DialogButton {
  i: number;
  label: string;
  labelSt?: readonly ConsoleSpan[] | null;
  a: DialogButtonAction;
  /** run_command: the command or template; the link, suggestion or text. */
  v?: string;
  /**
   * run_command (client 4.2.5): true, a template the game fills in; false, sent as it is. Absent: a
   * client 4.2.4, which fills in every command its own way.
   */
  tpl?: boolean;
  /** Its tooltip (client 4.2.5). */
  tip?: DialogText | null;
  /** Its width in the game's pixels (client 4.2.5). */
  w?: number | null;
}

/** What the game does after a press: closes the dialog, keeps it, or waits for the server. */
export type DialogAfter = "close" | "none" | "wait";

export interface Dialog {
  title: string;
  titleSt?: readonly ConsoleSpan[] | null;
  body: DialogPart[];
  inputs: DialogInput[];
  buttons: DialogButton[];
  /** The card may close it (the game lets the player close it, and closing runs no command). */
  cancel: boolean;
  /** In play (a dialog while the client is still joining runs no command). */
  play: boolean;
  cols: number;
  after: DialogAfter;
  /** The dialog's own exit button's label (client 4.2.5), when the card may close it. */
  exit: DialogText | null;
  /** The client cut a longer list (client 4.2.5): not all of the window shows. */
  cut: boolean;
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

const wholeIn = (value: unknown, min: number, max: number): number | null | undefined =>
  value === undefined
    ? null
    : Number.isInteger(value) && (value as number) >= min && (value as number) <= max
      ? (value as number)
      : undefined;

/** A label format with two `%s`, or `%1$s` and `%2$s`. */
function formatOf(value: unknown): string | null | undefined {
  if (value === undefined) return null;
  if (!word(value, DIALOG.format) || value === "") return undefined;
  const plain = value.split("%s").length - 1;
  const numbered = value.split("%1$s").length === 2 && value.split("%2$s").length === 2;
  return (plain === 2 && !value.includes("$")) || (numbered && plain === 0) ? value : undefined;
}

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
      const lines = wholeIn(value.lines, 1, 64);
      const height = wholeIn(value.height, 1, 512);
      if (lines === undefined || height === undefined) return null;
      return {
        ...head,
        kind: "text",
        max: max as number,
        multiline,
        lines: multiline ? lines : null,
        height: multiline ? height : null,
        initial,
      };
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
        options.push(
          st
            ? { id: option.id, label: option.label, labelSt: st }
            : { id: option.id, label: option.label },
        );
      }
      const initial = value.initial;
      if (typeof initial !== "string" || !options.some((o) => o.id === initial)) return null;
      return { ...head, kind: "option", options, initial };
    }
    case "range": {
      const { start, end, step, initial } = value;
      if (!finite(start) || !finite(end) || !finite(initial)) return null;
      if (step !== undefined && step !== null && (!finite(step) || step <= 0)) return null;
      const fmt = formatOf(value.fmt);
      if (fmt === undefined) return null;
      const f = Math.fround;
      const input: DialogRangeInput = {
        ...head,
        kind: "range",
        start: f(start),
        end: f(end),
        step: typeof step === "number" ? f(step) : null,
        initial: f(initial),
        fmt,
      };
      return sliderTakes(input.initial, input) ? input : null;
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
    if (value.tpl !== undefined) {
      if (typeof value.tpl !== "boolean") return null;
      // One the game would refuse never shows (the client does not send it either).
      if (value.tpl && parseTemplate(value.v) === null) return null;
      button.tpl = value.tpl;
    }
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
  if (value.tpl !== undefined && button.a !== "run_command") return null;
  // Client 4.2.5: its tooltip and width.
  if (value.tip !== undefined) {
    if (!word(value.tip, DIALOG.tooltip, DIALOG.tooltipLines) || value.tip === "") return null;
    button.tip = { text: value.tip, st: spansOf(value.tip, value.tipSt) };
  }
  const width = wholeIn(value.w, 1, 1024);
  if (width === undefined) return null;
  if (width !== null) button.w = width;
  return button;
}

function partOf(value: unknown): DialogPart | null {
  if (!isObject(value) || !word(value.text, DIALOG.bodyText, DIALOG.bodyLines)) return null;
  const part: DialogPart = { text: value.text, st: spansOf(value.text, value.st) };
  if (value.item !== undefined) {
    // Client 4.2.5: an item part, its stack as the hovers carry items.
    const item = safeInvItem(value.item);
    if (!item) return null;
    part.item = item;
    if (value.noTip === true) part.noTip = true;
    if (value.noDeco === true) part.noDeco = true;
    return part;
  }
  return value.text.length > 0 ? part : null;
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
  const body: DialogPart[] = [];
  if (!Array.isArray(value.body) || value.body.length > DIALOG.bodyParts) return null;
  for (const raw of value.body) {
    const part = partOf(raw);
    if (!part) return null;
    body.push(part);
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
  const after: DialogAfter =
    value.after === "none" || value.after === "wait" ? value.after : "close";
  if (value.after !== undefined && after === "close") return null;
  const cancel = value.cancel === true;
  let exit: DialogText | null = null;
  if (value.exit !== undefined) {
    if (!isObject(value.exit) || !word(value.exit.label, DIALOG.label)) return null;
    if (cancel && value.exit.label !== "") {
      exit = { text: value.exit.label, st: spansOf(value.exit.label, value.exit.labelSt) };
    }
  }
  return {
    title: value.title,
    titleSt,
    body,
    inputs,
    buttons,
    cancel,
    play: value.play !== false,
    cols,
    after,
    exit,
    cut: value.cut === true,
  };
}

/** An input's value as the player left it: text, `true`/`false`, an option's id, a number. */
export type DialogValues = Record<string, string | boolean | number>;

export function initialValues(dialog: Dialog): DialogValues {
  const out: DialogValues = {};
  for (const input of dialog.inputs) out[input.key] = input.initial;
  return out;
}

// --- Numbers as the game and the client write them -------------------------------------------

/** A float's shortest digits that read back as it (no leading or trailing zeros): value = d.ddd × 10^exp. */
function shortest(value: number): { negative: boolean; digits: string; exp: number } {
  const negative = value < 0 || Object.is(value, -0);
  const size = Math.abs(value);
  for (let precision = 1; precision <= 9; precision++) {
    const text = size.toExponential(precision - 1);
    if (Math.fround(Number(text)) !== size) continue;
    const [mantissa = "0", exp = "0"] = text.split("e");
    const digits = mantissa.replace(".", "").replace(/0+$/, "") || "0";
    return { negative, digits, exp: Number(exp) };
  }
  const [mantissa = "0", exp = "0"] = size.toExponential(8).split("e");
  return {
    negative,
    digits: mantissa.replace(".", "").replace(/0+$/, "") || "0",
    exp: Number(exp),
  };
}

/** Digits as a plain decimal; `wholeEnd` ends a whole number (Java writes ".0"). */
function plain(digits: string, exp: number, wholeEnd: string): string {
  const before = exp + 1;
  if (before <= 0) return `0.${"0".repeat(-before)}${digits}`;
  if (before >= digits.length) return `${digits}${"0".repeat(before - digits.length)}${wholeEnd}`;
  return `${digits.slice(0, before)}.${digits.slice(before)}`;
}

/** Java's `Float.toString`: plain from 10^-3 up to 10^7, else `d.dddE±n`. */
export function javaFloat(value: number): string {
  if (Number.isNaN(value)) return "NaN";
  if (!Number.isFinite(value)) return value > 0 ? "Infinity" : "-Infinity";
  if (value === 0) return Object.is(value, -0) ? "-0.0" : "0.0";
  const { negative, digits, exp } = shortest(Math.fround(value));
  const sign = negative ? "-" : "";
  if (exp >= -3 && exp < 7) return sign + plain(digits, exp, ".0");
  return `${sign}${digits[0]}.${digits.length > 1 ? digits.slice(1) : "0"}E${exp}`;
}

/** Java's `(int)` cast of a float: NaN is 0, the rest saturates. */
function javaInt(value: number): number {
  if (Number.isNaN(value)) return 0;
  return Math.max(-2147483648, Math.min(2147483647, Math.trunc(value)));
}

/** A slider's value in a template (the game's `valueToString`): a whole number as an integer, else Java's float text. */
export function sliderText(value: number): string {
  const whole = javaInt(value);
  return whole === value ? String(whole) : javaFloat(value);
}

/**
 * A number as an input's value is passed (the client's NumberToString, the machine's own text): a
 * whole number as an integer, else the shortest plain decimal that reads back as the same float.
 */
export function numberText(value: number): string {
  const v = Math.fround(value);
  if (Number.isInteger(v) && Math.abs(v) < 2147483648) return String(v);
  if (v === 0) return "0";
  const { negative, digits, exp } = shortest(v);
  return (negative ? "-" : "") + plain(digits, exp, "");
}

// --- A slider as the game's (NumberRangeInput.RangeInfo, in floats) -------------------------------

const f32 = Math.fround;

/** The game's place of a value on the slider (`scaledValueToSlider`). */
function sliderPlace(value: number, input: DialogRangeInput): number {
  return input.start === input.end
    ? 0.5
    : f32(f32(value - input.start) / f32(input.end - input.start));
}

const outOfRange = (value: number, input: DialogRangeInput): boolean => {
  const place = sliderPlace(value, input);
  return place < 0 || place > 1;
};

/** Java's `Math.round(float)`: the nearest whole number, a half up. */
const javaRound = (value: number): number => Math.floor(value + 0.5);

/** The value at slider place `place` (0 to 1) as the game computes it (`computeScaledValue`). */
export function sliderValue(place: number, input: DialogRangeInput): number {
  const lerped = f32(input.start + f32(f32(place) * f32(input.end - input.start)));
  if (input.step === null) return lerped;
  const n = javaRound(f32(f32(lerped - input.initial) / input.step));
  const value = f32(input.initial + f32(f32(n) * input.step));
  if (!outOfRange(value, input)) return value;
  return f32(input.initial + f32(f32(n - Math.sign(n)) * input.step));
}

/** Whether the slider can show `value`: in its range, and with a step on one of its steps. */
export function sliderTakes(value: number, input: DialogRangeInput): boolean {
  if (!Number.isFinite(value)) return false;
  const far = f32(input.start + f32(input.end - input.start));
  if (outOfRange(value, input) && value !== far) return false;
  if (input.step === null) return true;
  const n = javaRound(f32(f32(value - input.initial) / input.step));
  return f32(input.initial + f32(f32(n) * input.step)) === value;
}

/**
 * A stepped slider's steps as whole numbers `n` (value = initial + n × step) from `low` to `high`,
 * the ones inside its range.
 */
export function sliderSteps(input: DialogRangeInput): { low: number; high: number } {
  const step = input.step ?? 1;
  const at = (n: number) => f32(input.initial + f32(f32(n) * step));
  const edge = (value: number, inward: number) => {
    let n = javaRound((value - input.initial) / step);
    // Inside the range, nearest the edge.
    for (let i = 0; i < 4 && outOfRange(at(n), input); i++) n += inward;
    for (let i = 0; i < 4 && !outOfRange(at(n - inward), input); i++) n -= inward;
    return n;
  };
  const lo = Math.min(input.start, input.end);
  const hi = Math.max(input.start, input.end);
  return { low: edge(lo, 1), high: edge(hi, -1) };
}

/** A stepped slider's value at step `n`. */
export const stepValue = (input: DialogRangeInput, n: number): number =>
  f32(input.initial + f32(f32(n) * (input.step ?? 1)));

/** The slider's words as the game writes them: its format with the label and the value's text. */
export function sliderLabel(input: DialogRangeInput, value: number): string {
  const text = sliderText(value);
  const format = input.fmt ?? "%s: %s";
  if (format.includes("%1$s"))
    return format.split("%1$s").join(input.label).split("%2$s").join(text);
  const [first = "", second = "", third = ""] = format.split("%s");
  return `${first}${input.label}${second}${text}${third}`;
}

// --- Texts as the game's fields take them ----------------------------------------------------

/** A character the game's text field takes (`StringUtil.isAllowedChatCharacter`; a line break only when multiline). */
export const allowedInText = (c: string, multiline: boolean): boolean =>
  c === "\n" ? multiline : c !== "§" && c >= " " && c !== "\u007f";

/** What the game's field keeps of typed or pasted text: the characters it takes, within its lines. */
export function fieldText(input: DialogTextInput, typed: string): string {
  let kept = "";
  let lines = 1;
  for (const c of typed.replace(/\r\n?/g, "\n")) {
    if (!allowedInText(c, input.multiline)) continue;
    if (c === "\n") {
      if (input.lines !== null && lines >= input.lines) continue;
      lines++;
    }
    // Never past its length, never half of a pair.
    if (kept.length + c.length > input.max) break;
    kept += c;
  }
  return kept;
}

/** A text's value in a template, escaped as the game escapes it (`StringTag.escapeWithoutQuotes`). */
export function escapeText(value: string): string {
  let out = "";
  for (const c of value) {
    switch (c) {
      case '"':
      case "'":
      case "\\":
        out += `\\${c}`;
        break;
      case "\b":
        out += "\\b";
        break;
      case "\t":
        out += "\\t";
        break;
      case "\n":
        out += "\\n";
        break;
      case "\f":
        out += "\\f";
        break;
      case "\r":
        out += "\\r";
        break;
      default:
        out += c < " " ? `\\x${c.charCodeAt(0).toString(16).toUpperCase().padStart(2, "0")}` : c;
    }
  }
  return out;
}

const NAME_UNIT = /[\p{L}\p{Nd}]/u;

/**
 * A dynamic command's template as the game reads it (`StringTemplate.fromString`): its text and
 * `$(name)` variables, or null when the game refuses it (no variable, `$(` without `)`, a name of
 * other characters than letters, digits and `_`).
 */
export function parseTemplate(template: string): { segments: string[]; names: string[] } | null {
  const segments: string[] = [];
  const names: string[] = [];
  let start = 0;
  let dollar = template.indexOf("$");
  while (dollar !== -1) {
    if (template[dollar + 1] !== "(") {
      dollar = template.indexOf("$", dollar + 1);
      continue;
    }
    segments.push(template.slice(start, dollar));
    const close = template.indexOf(")", dollar + 1);
    if (close === -1) return null;
    const name = template.slice(dollar + 2, close);
    for (let i = 0; i < name.length; i++) {
      const unit = name.charAt(i);
      // One UTF-16 unit at a time, as the game reads it: half of a pair is no letter.
      const surrogate = unit >= "\ud800" && unit <= "\udfff";
      if (unit !== "_" && (surrogate || !NAME_UNIT.test(unit))) return null;
    }
    names.push(name);
    start = close + 1;
    dollar = template.indexOf("$", start);
  }
  if (start === 0) return null;
  if (start !== template.length) segments.push(template.slice(start));
  return { segments, names };
}

// --- Inputs and presses ----------------------------------------------------------------------

/**
 * The value an input gives the client, or null when the client would refuse it
 * (DialogManager.NormalizeInputValue): a text within its length that the game's field takes,
 * `true` or `false`, an option's id, a number the slider can show.
 */
export function inputText(input: DialogInput, value: unknown): string | null {
  switch (input.kind) {
    case "text":
      return typeof value === "string" &&
        value.length <= input.max &&
        clean(value, input.multiline) &&
        [...value].every((c) => allowedInText(c, input.multiline))
        ? value
        : null;
    case "bool":
      return typeof value === "boolean" ? String(value) : null;
    case "option":
      return typeof value === "string" && input.options.some((o) => o.id === value) ? value : null;
    case "range":
      return typeof value === "number" && sliderTakes(value, input) ? numberText(value) : null;
  }
}

/** What an input puts in a client 4.2.4 command template (its DialogManager.ToTemplateValue). */
function legacyValue(input: DialogInput, text: string): string {
  if (input.kind === "bool") return text === "true" ? input.onTrue : input.onFalse;
  if (input.kind === "text") return text.split("\\").join("\\\\").split('"').join('\\"');
  return text;
}

/** What an input puts in a client 4.2.5 template, as the game writes it. */
function exactValue(input: DialogInput, text: string): string {
  switch (input.kind) {
    case "bool":
      return text === "true" ? input.onTrue : input.onFalse;
    case "text":
      return escapeText(text);
    case "range":
      return sliderText(Math.fround(Number(text)));
    case "option":
      return text;
  }
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
 * The command a run_command button runs with these inputs, or null when it is no command the
 * contract allows (or the client is still joining): a client 4.2.5 template (`tpl`) filled in as
 * the game fills it (each `$(name)` once, never inside a value, one without an input as nothing), a
 * plain command as it is, and without `tpl` (client 4.2.4) each input in order, every `$(key)`
 * replaced.
 */
export function dialogCommand(
  dialog: Dialog,
  button: DialogButton,
  values: DialogValues,
): string | null {
  if (button.a !== "run_command" || !button.v || !dialog.play) return null;
  const inputs = dialogInputs(dialog, values);
  if (!inputs) return null;
  let command: string;
  if (button.tpl === false) {
    command = button.v;
  } else if (button.tpl === true) {
    const parsed = parseTemplate(button.v);
    if (!parsed) return null;
    command = "";
    for (let i = 0; i < parsed.names.length; i++) {
      command += parsed.segments[i] ?? "";
      const name = parsed.names[i];
      const input = dialog.inputs.find((candidate) => candidate.key === name);
      if (input) command += exactValue(input, inputs[input.key] ?? "");
    }
    if (parsed.segments.length > parsed.names.length) {
      command += parsed.segments[parsed.segments.length - 1] ?? "";
    }
  } else {
    command = button.v;
    for (const input of dialog.inputs) {
      // split/join, as the client's ordinal Replace: a `$` in a value is never a pattern.
      command = command.split(`$(${input.key})`).join(legacyValue(input, inputs[input.key] ?? ""));
    }
  }
  if (!command.startsWith("/")) command = `/${command}`;
  return command.length <= MAX_VALUE && safeClick({ a: "run_command", v: command })
    ? command
    : null;
}

/** The menus a client 4.2.5 reports: what the game's pause screen and quick actions key open. */
export type DialogMenu = "pause" | "quick";

/** A press of the card as the Worker takes it (`POST /accounts/:id/dialog`). */
export type DialogPress =
  | { rev: number; i: number; inputs: Record<string, string>; cmd?: string }
  | { rev: number; cancel: true }
  | { open: DialogMenu };

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

/** The menus as the machine says them: `pause` and `quick`, each a label of the contract. */
export type DialogMenuLabels = Partial<Record<DialogMenu, DialogText>>;

/** The machine's menus (`{pause?, quick?}`), each checked as a label; the ones out of shape left out. */
export function safeMenus(value: unknown): DialogMenuLabels {
  const out: DialogMenuLabels = {};
  if (!isObject(value)) return out;
  for (const menu of ["pause", "quick"] as const) {
    const raw = value[menu];
    if (isObject(raw) && word(raw.label, DIALOG.label) && raw.label !== "") {
      out[menu] = { text: raw.label, st: spansOf(raw.label, raw.labelSt) };
    }
  }
  return out;
}

/**
 * What the card says when a press is refused (`dialog_refused` with its `reason`): the dialog had
 * changed (its new card is already there), the menu is not there now, or any other reason, without
 * a detail.
 */
export function dialogRefusal(reason: unknown, lang: InventoryLang): string {
  const t = CONSOLE_TEXTS[lang];
  if (reason === "stale") return t.dialogChanged;
  if (reason === "menu") return t.dialogMenuGone;
  return t.dialogFailed;
}
