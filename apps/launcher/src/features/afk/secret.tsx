// One look for a secret line on every AFK surface (AFK-CLEAR: the desktop settings, the 7/24 Bulut
// settings, "Ayarları kopyala" and both consoles each drew their own). A secret is always a
// command-line field in monospace: a quiet padlock inside on the left, the command in front of the
// words (`/login`, taken from what is typed or given for a line typed again), only the rest masked,
// and the eye on the right that shows it on this computer. A secret that is kept is the same field,
// read-only (padlock, command, dots; the eye where the words can be had), its buttons beside it.
// The site draws its own field (a text field masked by CSS, so browsers' password managers stay
// away); the launcher's WebView has none, and a password field keeps the words out of copy and paste.
import { Eye, EyeSlash, LockSimple } from "@phosphor-icons/react";
import { cn, enter, IconButton, Input, leave } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import {
  type CSSProperties,
  forwardRef,
  type InputHTMLAttributes,
  type KeyboardEvent,
  type ReactNode,
  useState,
} from "react";
import { useTranslation } from "react-i18next";

/** The padlock's room inside the field (`pl-9`: 9 × 3.5 px). */
const LOCK_ROOM = "2.25rem";

const MONO = "font-mono text-[13px]";

/** A command and what follows it (`/login hunter2` → `/login`, `hunter2`), else null. */
export function splitCommand(line: string): { command: string; rest: string } | null {
  const parts = /^(\/\S+) ([\s\S]*)$/.exec(line);
  return parts?.[1] !== undefined && parts[2] !== undefined
    ? { command: parts[1], rest: parts[2] }
    : null;
}

type FieldProps = Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "value" | "onChange"> & {
  value: string;
  onChange: (text: string) => void;
  /**
   * The words are secret: padlock, mask and eye. Off, the same field is a plain one, so a line that
   * turns secret while it is typed keeps its caret (the input element never changes).
   */
  secret?: boolean;
  /**
   * The command in front of the words when `value` holds only what follows it (a kept line typed
   * again, "Ayarları kopyala"). Without it a secret line shows its own command in front.
   */
  command?: string | null;
  invalid?: boolean;
  /** Commands look like what is typed into the game; a reply is a sentence (a secret never is). */
  mono?: boolean;
  /** The eye that shows the words. */
  reveal?: boolean;
  /** The wrapper's classes (its width in a row). */
  wrapperClassName?: string;
};

export const SecretField = forwardRef<HTMLInputElement, FieldProps>(function SecretField(
  {
    value,
    onChange,
    secret = true,
    command = null,
    invalid,
    mono = true,
    reveal = true,
    className,
    wrapperClassName,
    style,
    onKeyDown,
    ...props
  },
  ref,
) {
  const { t } = useTranslation();
  const [shown, setShown] = useState(false);
  // Its own command in front, only the rest in the field (masked): a typed `/login x` reads like a
  // kept one. A Backspace at the start of the rest takes the space back.
  const own = secret && !command ? splitCommand(value) : null;
  const lead = secret ? (command ?? own?.command ?? null) : null;
  const text = own ? own.rest : value;
  const change = (next: string) => onChange(own ? `${own.command} ${next}` : next);
  const keyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const field = event.currentTarget;
    if (
      own &&
      event.key === "Backspace" &&
      field.selectionStart === 0 &&
      field.selectionEnd === 0
    ) {
      event.preventDefault();
      onChange(`${own.command}${own.rest}`);
      return;
    }
    onKeyDown?.(event);
  };
  // The command and one space before the words, in the field's monospace.
  const room: CSSProperties | undefined = lead
    ? { paddingLeft: `calc(${LOCK_ROOM} + ${lead.length + 1}ch)` }
    : undefined;
  return (
    <div className={cn("relative min-w-0 flex-1", wrapperClassName)}>
      {secret ? (
        <LockSimple
          size={15}
          weight="fill"
          aria-hidden
          className="pointer-events-none absolute top-1/2 left-3 z-10 -translate-y-1/2 text-ash-400"
        />
      ) : null}
      {lead ? (
        <span
          aria-hidden
          className={cn(
            "pointer-events-none absolute inset-y-0 left-9 z-10 flex items-center text-ash-300",
            MONO,
          )}
        >
          {lead}
        </span>
      ) : null}
      <Input
        ref={ref}
        {...props}
        value={text}
        type={secret && !shown ? "password" : "text"}
        spellCheck={false}
        autoComplete={secret ? "new-password" : "off"}
        data-1p-ignore=""
        data-lpignore="true"
        invalid={invalid}
        onChange={(event) => change(event.target.value)}
        onKeyDown={keyDown}
        style={room || style ? { ...style, ...room } : undefined}
        className={cn(
          "h-9",
          // A secret is a command line: its words in the monospace the room before them is counted in.
          secret || mono ? MONO : "text-[13.5px]",
          secret && "pl-9",
          secret && reveal && "pr-10",
          className,
        )}
      />
      {secret && reveal ? (
        <IconButton
          label={shown ? t("afkClear.secret.hide") : t("afkClear.secret.show")}
          icon={shown ? <EyeSlash size={16} /> : <Eye size={16} />}
          size="sm"
          className="absolute top-1/2 right-0.5 size-8 -translate-y-1/2"
          onClick={() => setShown((v) => !v)}
        />
      ) : null}
    </div>
  );
});

/**
 * A kept secret: the field's look, read-only. `prefix` is the command it starts with (or the
 * stored preview's name), `shown` the words while they are shown (they fade in over the dots),
 * `eye` the eye inside it where the words can be had, `children` its buttons beside it.
 */
export function SecretKept({
  label,
  prefix,
  invalid = false,
  shown = null,
  eye,
  overlay,
  children,
}: {
  /** Read out instead of the dots ("/login şifresi kayıtlı"). */
  label: string;
  prefix: string | null;
  invalid?: boolean;
  /** The words, for the few seconds they are shown (the cloud's eye). */
  shown?: string | null;
  /** The eye inside the field: the words come from somewhere else (the cloud's machine). */
  eye?: { busy: boolean; onToggle: () => void };
  /** Drawn over the field's bottom edge (the shown words' countdown). */
  overlay?: ReactNode;
  children?: ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <div className="flex min-w-0 flex-1 items-center gap-2">
      <div
        className={cn(
          "relative flex h-9 min-w-0 flex-1 items-center gap-2 overflow-hidden rounded-md border bg-obsidian-800/80 px-3 transition-[border-color] duration-150",
          invalid ? "border-flare-400/70" : "border-white/10",
          eye && "pr-10",
        )}
      >
        <LockSimple size={15} weight="fill" aria-hidden className="shrink-0 text-ash-400" />
        <span className="grid min-w-0 flex-1">
          <AnimatePresence initial={false}>
            {shown !== null ? (
              <motion.span
                key="shown"
                className={cn("truncate text-bone-50 [grid-area:1/1]", MONO)}
                data-selectable
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: enter(0.18) }}
                exit={{ opacity: 0, transition: leave(0.14) }}
              >
                {shown}
              </motion.span>
            ) : (
              <motion.span
                key="kept"
                className="flex min-w-0 items-center gap-2 [grid-area:1/1]"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: enter(0.18) }}
                exit={{ opacity: 0, transition: leave(0.14) }}
              >
                <span className="sr-only">{label}</span>
                {prefix ? (
                  <span aria-hidden className={cn("truncate text-ash-300", MONO)}>
                    {prefix}
                  </span>
                ) : null}
                <span aria-hidden className="font-mono text-[13px] tracking-widest text-ash-400">
                  ••••••
                </span>
              </motion.span>
            )}
          </AnimatePresence>
        </span>
        {eye ? (
          <IconButton
            label={shown !== null ? t("afkClear.secret.hide") : t("afkClear.secret.show")}
            icon={shown !== null ? <EyeSlash size={16} /> : <Eye size={16} />}
            size="sm"
            disabled={eye.busy}
            className="absolute top-1/2 right-0.5 size-8 -translate-y-1/2"
            onClick={eye.onToggle}
          />
        ) : null}
        {overlay}
      </div>
      {children}
    </div>
  );
}
