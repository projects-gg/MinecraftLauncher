// "Rapor et" (MOD1): one dialog for a player, a crew or a message. What is reported shows at the
// top (a crew invite can be about the crew or who sent it); then the fixed reasons, an optional
// note for moderators (200 characters) and, for players and senders, "Ayrıca engelle" (unticked).
// The reported person never learns about it. Sent: "Raporun iletildi."; a refusal stays in place.
import { Button, Dialog, Segmented, toast } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { RadioGroup } from "radix-ui";
import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { friendlyError } from "@/features/accounts/use-accounts";
import { Check } from "@/features/settings/cleanup/check";
import { api } from "@/lib/api";
import type { ReportReason } from "@/lib/api/social-types";
import { enter, leave } from "./motion";
import {
  canBlock,
  NOTE_MAX,
  noteLength,
  noteOf,
  REPORT_REASONS,
  reasonKey,
  reasonsFor,
} from "./report";
import { SocialAvatar } from "./social-face";
import { type ReportRequest, useSocialUi } from "./store";
import { CrewMark } from "./team-emblem";

function Subject({ request, inviter }: { request: ReportRequest; inviter: boolean }) {
  const { t } = useTranslation();
  const name = inviter && request.inviter ? request.inviter.name : request.name;
  const person = inviter && request.inviter ? request.inviter.person : request.person;
  const kind = inviter ? "player" : request.target.kind;
  return (
    <div className="flex items-start gap-3 rounded-[var(--radius-panel)] bg-obsidian-950/45 px-3.5 py-3 ring-1 ring-white/8">
      {kind === "crew" && request.crew ? (
        <CrewMark look={request.crew} size={32} />
      ) : (
        <SocialAvatar source={person ?? { name, avatar: "discord" }} size={32} />
      )}
      <div className="min-w-0 flex-1">
        <p className="truncate text-[14px] font-semibold text-bone-50">{name}</p>
        {/* From a plan card (FIX2): the plan's time and its note. */}
        {kind === "player" && !inviter && request.plan ? (
          <>
            <p className="mt-0.5 truncate text-[12.5px] text-ash-300">
              {t("fix2.report.plan", { when: request.plan.when })}
            </p>
            {request.plan.note ? (
              <p className="mt-1 line-clamp-2 text-[13px] leading-snug text-bone-100 break-words">
                <span className="sr-only">{t("fix2.note.label")}: </span>"{request.plan.note}"
              </p>
            ) : null}
          </>
        ) : null}
        {/* From a reaction's list (CHAT2): the reaction. */}
        {kind === "player" && !inviter && request.reaction ? (
          <p className="mt-0.5 truncate text-[12.5px] text-ash-300">
            {t("chat2.report.reaction", { emoji: request.reaction.e })}
          </p>
        ) : null}
        {kind === "message" && request.text ? (
          <p className="mt-1 line-clamp-3 text-[13px] leading-snug text-bone-100 break-words whitespace-pre-wrap">
            <span className="sr-only">{t("reports.message")}: </span>
            {request.text}
          </p>
        ) : null}
      </div>
    </div>
  );
}

export function ReportDialog() {
  const { t } = useTranslation();
  const request = useSocialUi((s) => s.report);
  const close = useSocialUi((s) => s.closeReport);
  const [inviter, setInviter] = useState(false);
  const [reason, setReason] = useState<ReportReason | null>(null);
  const [note, setNote] = useState("");
  const [block, setBlock] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const noteId = useId();
  const blockId = useId();

  // A new report starts empty.
  useEffect(() => {
    if (!request) return;
    setInviter(false);
    setReason(null);
    setNote("");
    setBlock(false);
    setError(null);
  }, [request]);

  const target = request && inviter && request.inviter ? request.inviter.target : request?.target;
  const kind = target?.kind ?? "player";
  const reasons = target ? reasonsFor(target) : REPORT_REASONS[kind];
  const fromPlan = target?.kind === "player" && Boolean(target.plan);
  const fromReaction = target?.kind === "player" && Boolean(target.reaction);
  const length = noteLength(note);
  const tooLong = length > NOTE_MAX;
  const ready = Boolean(target && reason && reasons.includes(reason)) && !tooLong && !busy;

  const send = async () => {
    if (!target || !reason) return;
    setBusy(true);
    setError(null);
    try {
      await api.social.report(target, reason, noteOf(note), canBlock(kind) && block);
      close();
      toast.success(t("reports.sent"));
    } catch (failure) {
      setError(friendlyError(failure, t("reports.failed")).message);
    } finally {
      setBusy(false);
    }
  };

  return (
    <Dialog
      open={request !== null}
      onOpenChange={(open) => {
        if (!open && !busy) close();
      }}
      title={t("reports.title")}
      description={t("reports.private")}
      closeLabel={t("common.close")}
      width={440}
      dismissible={!busy}
      morph
      footer={
        <>
          <Button variant="ghost" disabled={busy} onClick={close}>
            {t("reports.cancel")}
          </Button>
          <Button variant="primary" disabled={!ready} onClick={() => void send()}>
            {t("reports.send")}
          </Button>
        </>
      }
    >
      {request ? (
        <div className="flex flex-col gap-4">
          {request.inviter ? (
            <Segmented
              label={t("reports.what.label")}
              value={inviter ? "inviter" : "crew"}
              onValueChange={(value) => {
                setInviter(value === "inviter");
                setReason(null);
              }}
              options={[
                { value: "crew", label: t("reports.what.crew") },
                { value: "inviter", label: t("reports.what.inviter") },
              ]}
              className="self-start"
            />
          ) : null}
          <Subject request={request} inviter={inviter} />
          <RadioGroup.Root
            aria-label={t("reports.reasonsLabel")}
            value={reason ?? ""}
            onValueChange={(value) => setReason(value as ReportReason)}
            className="flex flex-col gap-1.5"
          >
            {reasons.map((option) => (
              <RadioGroup.Item
                key={option}
                value={option}
                className="group flex h-9 w-full cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] px-3 text-left text-[13.5px] text-bone-100 outline-none ring-1 ring-white/[0.07] transition-[background-color,box-shadow,color] duration-150 hover:bg-white/[0.04] focus-visible:ring-2 focus-visible:ring-gold-300 data-[state=checked]:bg-white/[0.06] data-[state=checked]:text-bone-50 data-[state=checked]:ring-white/25"
              >
                <span
                  aria-hidden
                  className="flex size-4 shrink-0 items-center justify-center rounded-full border border-white/25 transition-colors duration-150 group-data-[state=checked]:border-gold-400"
                >
                  <RadioGroup.Indicator className="size-2 rounded-full bg-gold-400" />
                </span>
                {t(reasonKey(kind, option, fromPlan, fromReaction))}
              </RadioGroup.Item>
            ))}
          </RadioGroup.Root>
          <div className="flex flex-col gap-1.5">
            <label htmlFor={noteId} className="text-[12.5px] font-medium text-ash-300">
              {t("reports.note")}
            </label>
            <textarea
              id={noteId}
              rows={2}
              value={note}
              onChange={(event) => setNote(event.target.value)}
              placeholder={t("reports.notePlaceholder")}
              aria-describedby={`${noteId}-help`}
              aria-invalid={tooLong || undefined}
              maxLength={NOTE_MAX * 4}
              className="min-h-[60px] w-full resize-none rounded-[var(--radius-panel)] border border-white/10 bg-obsidian-800/80 px-3 py-2 text-[13.5px] leading-5 text-bone-50 outline-none transition-[border-color,box-shadow] duration-150 placeholder:text-ash-500 hover:border-white/18 focus:border-gold-400/60 focus:ring-2 focus:ring-gold-400/20 aria-[invalid=true]:border-flare-400/60"
            />
            <p
              id={`${noteId}-help`}
              className="flex justify-between gap-3 text-[12px] text-ash-400"
            >
              <span>{t("reports.noteHelp")}</span>
              <span className={tooLong ? "text-flare-300 tabular-nums" : "tabular-nums"}>
                {length > NOTE_MAX - 40 ? `${length}/${NOTE_MAX}` : ""}
              </span>
            </p>
          </div>
          {canBlock(kind) ? (
            <div className="switch-row flex items-center gap-2.5">
              <Check id={blockId} checked={block} onCheckedChange={setBlock} />
              <label htmlFor={blockId} className="cursor-pointer text-[13.5px] text-bone-100">
                {t("reports.block")}
              </label>
            </div>
          ) : null}
          <AnimatePresence initial={false}>
            {error ? (
              <motion.p
                key={error}
                role="alert"
                className="text-[13px] text-flare-300"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: enter(0.18) }}
                exit={{ opacity: 0, transition: leave(0.12) }}
              >
                {error}
              </motion.p>
            ) : null}
          </AnimatePresence>
        </div>
      ) : null}
    </Dialog>
  );
}
