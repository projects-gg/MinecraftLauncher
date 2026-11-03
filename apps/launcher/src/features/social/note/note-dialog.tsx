// "Not ekle" / "Notu düzenle" (SOC-NOTE §3.5): a small dialog, mounted once by SocNoteHost, asked
// from the status menu or the player's own note on their card. The field opens with the note (or
// empty), the counter shows near the limit, and the reason a note cannot go stands in place under
// the field as the player types (a link, a phone number, too long), the service's refusal there
// too. "Ne kadar kalsın": "1 saat", "Bugün" (the default: the end of the player's day) or
// "Silinmesin". Enter saves, Esc or "Vazgeç" leaves; "Notu kaldır" takes a note off at once.
import { Button, cn, Dialog, Field, Input, Segmented, toast } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { announce, friendlyError } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import { enter, leave, useSocialMotion } from "../motion";
import {
  cleanNote,
  lengthOf,
  NOTE_COUNT_FROM,
  NOTE_LENGTHS,
  NOTE_MAX,
  type NoteLength,
  noteLength,
  noteProblem,
  noteUntil,
  useOwnNote,
} from "./notes";

const useNoteAsk = create<{ open: boolean }>()(() => ({ open: false }));

/** Opens the dialog (after the menu that asked has let go of the focus). */
export function askNote() {
  window.setTimeout(() => useNoteAsk.setState({ open: true }), 0);
}

/** The reason a note cannot go, or the service's refusal, in place under the field. */
function NoteLine({ id, text }: { id: string; text: string | null }) {
  const { reduce } = useSocialMotion();
  return (
    <p id={id} className="relative min-h-[20px] px-0.5 pt-1.5 text-[12px] leading-none">
      <AnimatePresence initial={false}>
        {text ? (
          <motion.span
            key={text}
            role="alert"
            className="block text-flare-300"
            initial={{ opacity: 0, transform: reduce ? "none" : "translateY(-2px)" }}
            animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.18) }}
            exit={{ opacity: 0, transition: leave(0.12) }}
          >
            {text}
          </motion.span>
        ) : null}
      </AnimatePresence>
    </p>
  );
}

export function NoteDialog() {
  const { t } = useTranslation();
  const open = useNoteAsk((s) => s.open);
  const note = useOwnNote();
  const [text, setText] = useState("");
  const [length, setLength] = useState<NoteLength>("today");
  const [busy, setBusy] = useState(false);
  const [refused, setRefused] = useState<string | null>(null);
  const fieldId = useId();
  const lineId = useId();
  const lengthId = useId();
  const input = useRef<HTMLInputElement>(null);
  // Only when the dialog opens: typing keeps what was typed.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the note is read once per opening
  useEffect(() => {
    if (!open) return;
    setText(note?.text ?? "");
    setLength(lengthOf(note, Date.now()));
    setRefused(null);
    setBusy(false);
  }, [open]);
  const close = () => useNoteAsk.setState({ open: false });

  const clean = cleanNote(text);
  const problem = noteProblem(text);
  const count = noteLength(text);
  const showCount = count >= NOTE_COUNT_FROM;
  const until = noteUntil(length, Date.now());
  const unchanged =
    clean === (note?.text ?? "") && (note === undefined || lengthOf(note, Date.now()) === length);
  const ready = clean.length > 0 && problem === null && !unchanged && !busy;
  const line = problem ? t(`socNote.note.${problem}`) : refused;

  const send = async (next: string, at: number, done: string) => {
    setBusy(true);
    setRefused(null);
    try {
      await api.social.setStatusNote(next, at);
      announce(done);
      toast.success(done);
      close();
    } catch (error) {
      setRefused(friendlyError(error, t("social.actions.failed")).message);
      input.current?.focus();
    } finally {
      setBusy(false);
    }
  };
  const save = () => {
    if (!ready) return;
    void send(clean, until, t("socNote.note.saved"));
  };
  const remove = () => {
    if (busy) return;
    void send("", 0, t("socNote.note.removed"));
  };

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next) close();
      }}
      title={t("socNote.note.title")}
      description={t("socNote.note.hint")}
      closeLabel={t("common.close")}
      width={420}
      footer={
        <>
          {note ? (
            <Button variant="ghost" className="mr-auto" disabled={busy} onClick={remove}>
              {t("socNote.note.remove")}
            </Button>
          ) : null}
          <Button variant="ghost" onClick={close}>
            {t("social.actions.cancel")}
          </Button>
          <Button variant="primary" disabled={!ready} onClick={save}>
            {t("socNote.note.save")}
          </Button>
        </>
      }
    >
      <form
        onSubmit={(event) => {
          event.preventDefault();
          save();
        }}
      >
        <Field label={t("socNote.note.label")} htmlFor={fieldId}>
          <div className="relative">
            <Input
              ref={input}
              id={fieldId}
              autoFocus
              value={text}
              placeholder={t("socNote.note.placeholder")}
              maxLength={NOTE_MAX * 5}
              enterKeyHint="done"
              invalid={Boolean(line)}
              aria-describedby={line || showCount ? lineId : undefined}
              onChange={(event) => {
                setText(event.target.value);
                setRefused(null);
              }}
              className={cn(
                "transition-[border-color,box-shadow,padding]",
                showCount && "pr-[52px]",
              )}
            />
            <span
              aria-hidden={!showCount}
              className={cn(
                "pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 text-[11.5px] tabular-nums transition-opacity duration-150",
                showCount ? "opacity-100" : "opacity-0",
                count > NOTE_MAX ? "text-flare-300" : "text-ash-400",
              )}
            >
              {t("socNote.note.count", { count })}
            </span>
          </div>
          <NoteLine id={lineId} text={line} />
        </Field>
        <p id={lengthId} className="mt-1 mb-2 text-[12.5px] font-medium text-ash-300">
          {t("socNote.note.lengthLabel")}
        </p>
        <Segmented
          value={length}
          onValueChange={setLength}
          label={t("socNote.note.lengthLabel")}
          describedBy={lengthId}
          options={NOTE_LENGTHS.map((value) => ({
            value,
            label: t(`socNote.note.lengths.${value}`),
          }))}
        />
      </form>
    </Dialog>
  );
}
