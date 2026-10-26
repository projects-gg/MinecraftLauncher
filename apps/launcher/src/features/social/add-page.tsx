// "Arkadaş ekle": the player's own code (copy, share link, QR, a new code) and a friend's code:
// type or paste it, see who it is, then send. Adding works by code only; nobody can be searched.
import {
  ArrowsClockwise,
  Check,
  Copy,
  LinkSimple,
  MagnifyingGlass,
  WarningCircle,
} from "@phosphor-icons/react";
import { Button, Skeleton } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type FormEvent, type ReactNode, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce, friendlyError } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { CodeLookup, SocialMe } from "@/lib/api/social-types";
import { attempt, copyText } from "./actions";
import { formatCodeInput, normalizeCode } from "./model";
import { rise, swap, useSocialMotion } from "./motion";
import { NameWithMark } from "./person-mark";
import { QrCode } from "./qr-code";
import { SocialAvatar } from "./social-face";
import { useSocialUi } from "./store";

/** A copy button whose label turns into "Kopyalandı" for a moment. */
function CopyButton({
  text,
  label,
  icon,
  done,
}: {
  text: string;
  label: string;
  icon: ReactNode;
  done: string;
}) {
  const { reduce } = useSocialMotion();
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );
  return (
    <Button
      size="sm"
      variant="secondary"
      onClick={() => {
        void copyText(text).then((ok) => {
          if (!ok) return;
          setCopied(true);
          announce(done);
          if (timer.current) clearTimeout(timer.current);
          timer.current = setTimeout(() => setCopied(false), 1600);
        });
      }}
    >
      <span className="relative inline-flex items-center">
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span
            key={String(copied)}
            className="flex items-center gap-1.5"
            {...swap(reduce, 0.16)}
          >
            {copied ? <Check size={14} weight="bold" className="text-jade-400" /> : icon}
            {copied ? done : label}
          </motion.span>
        </AnimatePresence>
      </span>
    </Button>
  );
}

function CodeCard({ me }: { me: SocialMe }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);

  const rotate = async () => {
    setBusy(true);
    const code = await attempt(() => api.social.rotateCode());
    setBusy(false);
    setAsking(false);
    if (code) announce(t("social.add.rotated"));
  };

  return (
    <section className="rounded-[var(--radius-panel)] bg-obsidian-950/40 p-4 ring-1 ring-white/8">
      <div className="flex gap-4">
        <AnimatePresence mode="popLayout" initial={false}>
          {me.qr ? (
            <motion.div key={me.friendCode} {...swap(reduce, 0.24)}>
              <QrCode qr={me.qr} size={112} label={t("social.add.qr")} />
            </motion.div>
          ) : (
            <Skeleton className="size-[112px] shrink-0" />
          )}
        </AnimatePresence>
        <div className="flex min-w-0 flex-1 flex-col">
          <p className="text-[12.5px] font-medium text-ash-400">{t("social.add.yourCode")}</p>
          <p className="relative mt-1 h-8 overflow-hidden font-display text-[23px] leading-8 font-semibold tracking-[0.04em] text-bone-50 tabular-nums">
            <AnimatePresence mode="popLayout" initial={false}>
              <motion.span
                key={me.friendCode}
                className="absolute inset-0"
                data-selectable
                {...swap(reduce, 0.28)}
              >
                {me.friendCode}
              </motion.span>
            </AnimatePresence>
          </p>
          <div className="mt-auto flex flex-wrap gap-2 pt-3">
            <CopyButton
              text={me.friendCode}
              label={t("social.add.copy")}
              done={t("social.add.copied")}
              icon={<Copy size={14} />}
            />
            {me.shareLink ? (
              <CopyButton
                text={me.shareLink}
                label={t("social.add.copyLink")}
                done={t("social.add.linkCopied")}
                icon={<LinkSimple size={14} />}
              />
            ) : null}
          </div>
        </div>
      </div>
      <div className="mt-3 border-t border-white/7 pt-2">
        <AnimatePresence mode="popLayout" initial={false}>
          {asking ? (
            <motion.div key="ask" className="flex items-center gap-2" {...rise(reduce, 4, 0.2)}>
              <p className="min-w-0 flex-1 text-[13px] text-bone-100">
                {t("social.add.rotateAsk")}
              </p>
              <Button size="sm" variant="ghost" onClick={() => setAsking(false)}>
                {t("social.actions.cancel")}
              </Button>
              <Button size="sm" variant="secondary" disabled={busy} onClick={() => void rotate()}>
                {t("social.add.rotate")}
              </Button>
            </motion.div>
          ) : (
            <motion.div key="idle" {...rise(reduce, 4, 0.2)}>
              <Button
                size="sm"
                variant="ghost"
                icon={<ArrowsClockwise size={14} />}
                onClick={() => setAsking(true)}
              >
                {t("social.add.rotate")}
              </Button>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </section>
  );
}

type Found =
  | { kind: "idle" }
  | { kind: "looking" }
  | { kind: "found"; lookup: CodeLookup; sent?: "requested" | "friends" }
  | { kind: "error"; message: string };

function ResultCard({
  found,
  onSent,
}: {
  found: Extract<Found, { kind: "found" }>;
  onSent: (outcome: "requested" | "friends") => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const [busy, setBusy] = useState(false);
  const { lookup, sent } = found;
  const send = async () => {
    setBusy(true);
    const outcome = await attempt(() => api.social.sendRequest(lookup.code));
    setBusy(false);
    if (outcome) {
      onSent(outcome);
      announce(outcome === "friends" ? t("social.add.nowFriends") : t("social.add.sent"));
    }
  };
  const act = async (work: () => Promise<void>) => {
    setBusy(true);
    const done = await attempt(work);
    setBusy(false);
    if (done !== undefined) onSent(lookup.relation === "incoming" ? "friends" : "requested");
  };
  const status = sent
    ? sent === "friends"
      ? t("social.add.nowFriends")
      : t("social.add.sent")
    : lookup.relation === "none"
      ? null
      : t(`social.add.relation.${lookup.relation}`);
  return (
    <div className="flex items-center gap-3.5 rounded-[var(--radius-panel)] bg-white/[0.04] p-3 ring-1 ring-white/8">
      <SocialAvatar source={lookup.profile} size={48} />
      <div className="min-w-0 flex-1">
        <NameWithMark
          name={lookup.profile.name}
          mark={lookup.profile.mark}
          className="text-[15px] font-semibold text-bone-50"
        />
        <p className="truncate text-[12.5px] text-ash-400">@{lookup.profile.username}</p>
        <div className="relative h-5">
          <AnimatePresence mode="popLayout" initial={false}>
            {status ? (
              <motion.p
                key={status}
                className="flex items-center gap-1.5 text-[12.5px] leading-5 text-jade-400"
                {...swap(reduce)}
              >
                {sent || lookup.relation === "friend" ? (
                  <Check size={13} weight="bold" aria-hidden />
                ) : null}
                <span className={sent || lookup.relation === "friend" ? "" : "text-ash-300"}>
                  {status}
                </span>
              </motion.p>
            ) : null}
          </AnimatePresence>
        </div>
      </div>
      {sent ? null : lookup.relation === "none" ? (
        <Button variant="primary" size="sm" disabled={busy} onClick={() => void send()}>
          {t("social.add.send")}
        </Button>
      ) : lookup.relation === "incoming" ? (
        <Button
          variant="primary"
          size="sm"
          disabled={busy}
          onClick={() => void act(() => api.social.accept(lookup.profile.id))}
        >
          {t("social.actions.accept")}
        </Button>
      ) : lookup.relation === "blocked" ? (
        <Button
          variant="secondary"
          size="sm"
          disabled={busy}
          onClick={() => void act(() => api.social.unblock(lookup.profile.id))}
        >
          {t("social.actions.unblock")}
        </Button>
      ) : null}
    </div>
  );
}

function FindForm() {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const inputId = useId();
  const errorId = useId();
  const inputRef = useRef<HTMLInputElement>(null);
  /** The next change comes from a paste: a complete code is looked up at once. */
  const pasted = useRef(false);
  const [value, setValue] = useState("");
  const [invalid, setInvalid] = useState(false);
  const [found, setFound] = useState<Found>({ kind: "idle" });
  const takePrefill = useSocialUi((s) => s.takePrefill);
  const prefill = useSocialUi((s) => s.prefillCode);

  const look = async (input: string) => {
    const code = normalizeCode(input);
    if (!code) {
      setInvalid(true);
      inputRef.current?.focus();
      return;
    }
    setInvalid(false);
    setFound({ kind: "looking" });
    try {
      const lookup = await api.social.lookup(code);
      setFound({ kind: "found", lookup });
    } catch (error) {
      setFound({
        kind: "error",
        message: friendlyError(error, t("social.actions.failed")).message,
      });
    }
  };

  // A `projects://friend/<code>` link (from a QR page) fills the code and looks it up.
  // biome-ignore lint/correctness/useExhaustiveDependencies: runs when a new link arrives
  useEffect(() => {
    const code = takePrefill();
    if (!code) return;
    const formatted = formatCodeInput(code);
    setValue(formatted);
    void look(formatted);
  }, [prefill]);

  const onSubmit = (event: FormEvent) => {
    event.preventDefault();
    void look(value);
  };

  return (
    <section className="mt-6">
      <form onSubmit={onSubmit}>
        <label htmlFor={inputId} className="mb-1.5 block text-[12.5px] font-medium text-ash-400">
          {t("social.add.friendCode")}
        </label>
        <div className="flex gap-2">
          <input
            ref={inputRef}
            id={inputId}
            value={value}
            onPaste={() => {
              pasted.current = true;
            }}
            onChange={(event) => {
              const formatted = formatCodeInput(event.target.value);
              setValue(formatted);
              setInvalid(false);
              const fromPaste = pasted.current;
              pasted.current = false;
              if (fromPaste && normalizeCode(formatted)) {
                void look(formatted);
                return;
              }
              if (found.kind !== "looking") setFound({ kind: "idle" });
            }}
            placeholder="PRJ-XXXX-XXXX"
            spellCheck={false}
            autoComplete="off"
            autoCapitalize="characters"
            aria-invalid={invalid}
            aria-describedby={invalid ? errorId : undefined}
            className="h-10 min-w-0 flex-1 rounded-md border border-white/10 bg-obsidian-800/80 px-3 font-display text-[16px] font-medium tracking-[0.06em] text-bone-50 tabular-nums transition-colors outline-none placeholder:text-ash-500 hover:border-white/16 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-300/25 aria-[invalid=true]:border-flare-400/70"
          />
          <Button
            type="submit"
            variant="secondary"
            icon={<MagnifyingGlass size={15} />}
            disabled={found.kind === "looking"}
          >
            {t("social.add.find")}
          </Button>
        </div>
        <AnimatePresence initial={false}>
          {invalid ? (
            <motion.p
              id={errorId}
              role="alert"
              className="mt-2 text-[12.5px] text-flare-400"
              {...rise(reduce, 4, 0.2)}
            >
              {t("social.add.invalid")}
            </motion.p>
          ) : null}
        </AnimatePresence>
      </form>
      <div className="mt-3">
        <AnimatePresence mode="popLayout" initial={false}>
          {found.kind === "looking" ? (
            <motion.div key="looking" aria-hidden {...rise(reduce, 4, 0.2)}>
              <div className="flex items-center gap-3.5 rounded-[var(--radius-panel)] p-3 ring-1 ring-white/6">
                <Skeleton className="size-12 rounded-[var(--radius-head)]" />
                <div className="flex-1 space-y-2">
                  <Skeleton className="h-4 w-32" />
                  <Skeleton className="h-3 w-24" />
                </div>
              </div>
            </motion.div>
          ) : found.kind === "found" ? (
            <motion.div key={`found:${found.lookup.code}`} {...rise(reduce, 6, 0.26)}>
              <ResultCard found={found} onSent={(sent) => setFound({ ...found, sent })} />
            </motion.div>
          ) : found.kind === "error" ? (
            <motion.p
              key="error"
              role="alert"
              className="flex items-start gap-2 px-1 text-[13px] text-flare-400"
              {...rise(reduce, 4, 0.2)}
            >
              <WarningCircle size={16} weight="fill" className="mt-px shrink-0" aria-hidden />
              {found.message}
            </motion.p>
          ) : null}
        </AnimatePresence>
      </div>
    </section>
  );
}

export function AddPage() {
  const me = useSocialUi((s) => s.view.me);
  return (
    <div className="pt-1">
      {me ? <CodeCard me={me} /> : null}
      <FindForm />
    </div>
  );
}
