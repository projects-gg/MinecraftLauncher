// 7/24 Bulut moderation on the cards and in the settings (README "Moderation player API"): the
// card's one sentence with "İtiraz et" (it opens the site's appeal page in the browser), the line
// under a paused row (that it is stopped, why, what runs it again, and the appeal) and "Yanlışsa
// bildir" under a row a save refused for its content.
import { ArrowSquareOut, ShieldWarning } from "@phosphor-icons/react";
import type { TFunction } from "i18next";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { afkFailed } from "../errors";
import { type CloudAccount, type CloudReportOutcome, cloudApi } from "./api";
import { categoryText, moderationLine } from "./moderation";

/** Opens the site's appeal page for the account in the browser. */
export function openAppeal(accountId: string, t: TFunction) {
  void cloudApi
    .open("appeal", accountId)
    .catch((error: unknown) => afkFailed(error, t("cloudAfk.actions.failed")));
}

/** "İtiraz et" as a quiet link that leaves for the browser (as "Süreyi uzat", not gold). */
function AppealLink({ accountId }: { accountId: string }) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      onClick={() => openAppeal(accountId, t)}
      className="inline-flex shrink-0 cursor-pointer items-center gap-1 rounded-sm font-medium text-bone-100 underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
    >
      {t("afkMod.card.appeal")}
      <ArrowSquareOut size={12} weight="bold" aria-hidden />
    </button>
  );
}

/** The card's line while a moderation step holds (nothing in observe mode or for a stop). */
export function ModerationLine({ account }: { account: CloudAccount }) {
  const { t } = useTranslation();
  const line = moderationLine(account, t);
  if (!line) return null;
  return (
    <div className="mt-2 text-[12.5px]">
      <p className="flex min-h-6 items-center gap-2">
        <ShieldWarning size={15} className="shrink-0 text-gold-300" aria-hidden />
        <span className="min-w-0 truncate text-bone-100" title={line.text}>
          {line.text}
        </span>
        {line.canAppeal ? <AppealLink accountId={account.id} /> : null}
      </p>
      {line.appealOpen ? (
        <p className="pl-[23px] text-[12px] text-ash-300">{t("afkMod.card.appealOpen")}</p>
      ) : null}
    </div>
  );
}

/** Under a row a case paused: that it is stopped, why, what runs it again; the appeal. */
export function PausedLine({ category, account }: { category: string; account: CloudAccount }) {
  const { t } = useTranslation();
  return (
    <p className="flex flex-wrap items-center gap-x-3 gap-y-1 text-[12.5px] text-bone-100">
      <span className="inline-flex items-center gap-1.5">
        <ShieldWarning size={14} aria-hidden className="shrink-0 text-gold-300" />
        {t("afkMod.paused.line", { kind: categoryText(category, t) })}
      </span>
      {account.moderation?.appeal.can ? <AppealLink accountId={account.id} /> : null}
    </p>
  );
}

/** One of "Yanlışsa bildir"'s answers in the link's place: quiet, announced. */
function ReportAnswer({ kind }: { kind: CloudReportOutcome["kind"] }) {
  const { t } = useTranslation();
  return (
    <p role="status" className="text-[12.5px] text-ash-300">
      {t(`afkMod.report.${kind}`)}
    </p>
  );
}

/**
 * Under a row a save refused for its content (README "Saving" → Refusal reports): "Yanlışsa
 * bildir" as a quiet underlined link (it stays here, so no arrow; the sheet's one gold is "Kaydet"),
 * then one line in its place: sent, three today already, or the text saves now (`onSaveable`: the
 * refusal is lifted where the save is held). A failure is the usual toast, and the link stays.
 */
export function RefusalReport({
  accountId,
  text,
  onSaveable,
}: {
  accountId: string;
  text: string;
  onSaveable?: () => void;
}) {
  const { t } = useTranslation();
  const [sending, setSending] = useState(false);
  const [answer, setAnswer] = useState<CloudReportOutcome["kind"] | null>(null);
  const send = async () => {
    if (sending) return;
    setSending(true);
    try {
      const { kind } = await cloudApi.reportRefusal(accountId, text);
      setAnswer(kind);
      if (kind === "notRefused") onSaveable?.();
    } catch (error) {
      afkFailed(error, t("cloudAfk.actions.failed"));
    } finally {
      setSending(false);
    }
  };
  if (answer) return <ReportAnswer kind={answer} />;
  return (
    <p className="text-[12.5px]">
      <button
        type="button"
        onClick={() => void send()}
        disabled={sending}
        className="cursor-pointer rounded-sm font-medium text-bone-100 underline decoration-white/35 underline-offset-[3px] outline-none hover:decoration-bone-100 focus-visible:ring-2 focus-visible:ring-gold-300 disabled:cursor-default disabled:opacity-60"
      >
        {sending ? t("afkMod.report.sending") : t("afkMod.report.link")}
      </button>
    </p>
  );
}

/**
 * A command's or a reply's line under its text: "Yanlışsa bildir" while a save refused `refused`
 * for its content; once the service says that text saves now, its refusal is cleared
 * (`onSaveable`: the mark, the "!" and the bar's line go, Kaydet opens) and "Bu satır artık
 * kaydedilebilir." stays for as long as the row keeps that text (`current`). Always rendered, so
 * what it was told outlives the refusal it answered.
 */
export function ReportLine({
  accountId,
  refused,
  current,
  onSaveable,
}: {
  accountId: string;
  refused: string | null;
  current: string | null;
  onSaveable: () => void;
}) {
  const [saves, setSaves] = useState<string | null>(null);
  if (refused) {
    return (
      <RefusalReport
        accountId={accountId}
        text={refused}
        onSaveable={() => {
          setSaves(refused);
          onSaveable();
        }}
      />
    );
  }
  return saves !== null && saves === current ? <ReportAnswer kind="notRefused" /> : null;
}
