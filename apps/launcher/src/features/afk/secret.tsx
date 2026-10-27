// One look for a secret line on every AFK surface (AFK-CLEAR: the desktop settings, the 7/24 Bulut
// settings, "Ayarları kopyala" and both consoles each drew their own). A secret being typed is a
// field with a quiet padlock inside on the left, the command in front of the words when it is known
// (`/login`: only the password is typed again), the words masked, and an eye on the right that shows
// them on this computer only. A secret that is kept is the same field, read-only (padlock, command,
// dots), its buttons beside it. The site draws its own field (a text field masked by CSS, so browsers'
// password managers stay away); the launcher's WebView has none, and a password field keeps the
// words out of copy and paste.
import { Eye, EyeSlash, LockSimple } from "@phosphor-icons/react";
import { cn, enter, IconButton, Input, leave } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import {
  type CSSProperties,
  forwardRef,
  type InputHTMLAttributes,
  type ReactNode,
  useState,
} from "react";
import { useTranslation } from "react-i18next";

/** The padlock's room inside the field (`pl-9`: 9 × 3.5 px). */
const LOCK_ROOM = "2.25rem";

type FieldProps = Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "value" | "onChange"> & {
  value: string;
  onChange: (text: string) => void;
  /**
   * The words are secret: padlock, mask and eye. Off, the same field is a plain one, so a line that
   * turns secret while it is typed keeps its caret (the input element never changes).
   */
  secret?: boolean;
  /** The command in front of the words (`/login`), shown but never typed. */
  prefix?: string | null;
  invalid?: boolean;
  /** Commands look like what is typed into the game; a reply is a sentence. */
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
    prefix = null,
    invalid,
    mono = true,
    reveal = true,
    className,
    wrapperClassName,
    style,
    ...props
  },
  ref,
) {
  const { t } = useTranslation();
  const [shown, setShown] = useState(false);
  const lead = secret && prefix ? prefix : null;
  // The command and one space before the words, in the field's own monospace.
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
          className="pointer-events-none absolute inset-y-0 left-9 z-10 flex items-center font-mono text-[13px] text-ash-300"
        >
          {lead}
        </span>
      ) : null}
      <Input
        ref={ref}
        {...props}
        value={value}
        type={secret && !shown ? "password" : "text"}
        spellCheck={false}
        autoComplete={secret ? "new-password" : "off"}
        data-1p-ignore=""
        data-lpignore="true"
        invalid={invalid}
        onChange={(event) => onChange(event.target.value)}
        style={room || style ? { ...style, ...room } : undefined}
        className={cn(
          "h-9",
          mono ? "font-mono text-[13px]" : "text-[13.5px]",
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
 * `children` its buttons beside it.
 */
export function SecretKept({
  label,
  prefix,
  invalid = false,
  mono = true,
  shown = null,
  overlay,
  children,
}: {
  /** Read out instead of the dots ("/login şifresi kayıtlı"). */
  label: string;
  prefix: string | null;
  invalid?: boolean;
  mono?: boolean;
  /** The words, for the few seconds they are shown (the cloud's "Göster"). */
  shown?: string | null;
  /** Drawn over the field's bottom edge (the shown words' countdown). */
  overlay?: ReactNode;
  children?: ReactNode;
}) {
  const face = mono ? "font-mono text-[13px]" : "text-[13.5px]";
  return (
    <div className="flex min-w-0 flex-1 items-center gap-2">
      <div
        className={cn(
          "relative flex h-9 min-w-0 flex-1 items-center gap-2 overflow-hidden rounded-md border bg-obsidian-800/80 px-3 transition-[border-color] duration-150",
          invalid ? "border-flare-400/70" : "border-white/10",
        )}
      >
        <LockSimple size={15} weight="fill" aria-hidden className="shrink-0 text-ash-400" />
        <span className="grid min-w-0 flex-1">
          <AnimatePresence initial={false}>
            {shown !== null ? (
              <motion.span
                key="shown"
                className={cn("truncate text-bone-50 [grid-area:1/1]", face)}
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
                  <span aria-hidden className={cn("truncate text-ash-300", face)}>
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
        {overlay}
      </div>
      {children}
    </div>
  );
}
