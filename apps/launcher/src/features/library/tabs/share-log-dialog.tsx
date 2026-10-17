import { ArrowSquareOut, Copy, Globe, WarningCircle } from "@phosphor-icons/react";
import { Button, Dialog, Input, Progress, toast } from "@pl/ui";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { copyErrorDetails, errorCode } from "@/features/content/jobs";
import { api, type GameLogSession, type SharedLog } from "@/lib/api";
import { copyText } from "@/lib/clipboard";

type ShareState =
  | { kind: "confirm" }
  | { kind: "uploading" }
  | { kind: "done"; link: SharedLog }
  | { kind: "error"; message: string; error: unknown };

/** Links made in this run, so sharing the same unchanged log twice does not upload it again. */
const sharedLinks = new Map<string, SharedLog>();
const linkKey = (instanceId: string, s: GameLogSession) =>
  `${instanceId}/${s.source}/${s.fileName}/${s.sizeBytes}/${s.updatedAt ?? ""}`;

/**
 * "Paylaş": asks first (the log becomes public on mclo.gs), uploads the masked log, then shows
 * the link with copy and open actions.
 */
export function ShareLogDialog({
  instanceId,
  session,
  open,
  onOpenChange,
}: {
  instanceId: string;
  session: GameLogSession;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  const { t } = useTranslation();
  const [state, setState] = useState<ShareState>({ kind: "confirm" });

  useEffect(() => {
    if (!open) return;
    const known = sharedLinks.get(linkKey(instanceId, session));
    setState(known ? { kind: "done", link: known } : { kind: "confirm" });
  }, [open, instanceId, session]);

  const upload = async () => {
    setState({ kind: "uploading" });
    try {
      const link = await api.library.shareLog(instanceId, session.fileName, session.source);
      sharedLinks.set(linkKey(instanceId, session), link);
      setState({ kind: "done", link });
    } catch (error) {
      // A failure the backend explained keeps its words; anything else gets ours.
      const message =
        error instanceof Error && error.message && errorCode(error)
          ? error.message
          : t("instance.logs.share.failed");
      setState({ kind: "error", message, error });
    }
  };

  const copyLink = async (url: string) => {
    try {
      await copyText(url);
      toast.success(t("instance.logs.share.copied"));
    } catch {
      toast.error(t("instance.logs.share.copyFailed"));
    }
  };

  const close = () => onOpenChange(false);
  const done = state.kind === "done" ? state.link : null;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        // Closing while uploading is fine: the result is kept for the next open.
        onOpenChange(next);
      }}
      title={done ? t("instance.logs.share.readyTitle") : t("instance.logs.share.title")}
      dismissOnOutsideClick={false}
      description={done ? t("instance.logs.share.readyBody") : t("instance.logs.share.body")}
      closeLabel={t("common.close")}
      width={480}
      footer={
        done ? (
          <>
            <Button
              variant="ghost"
              icon={<ArrowSquareOut size={16} />}
              onClick={() => void api.openUrl(done.url)}
            >
              {t("instance.logs.share.open")}
            </Button>
            <Button
              variant="primary"
              icon={<Copy size={16} />}
              onClick={() => void copyLink(done.url)}
            >
              {t("instance.logs.share.copy")}
            </Button>
          </>
        ) : state.kind === "error" ? (
          <>
            <Button variant="ghost" onClick={close}>
              {t("common.close")}
            </Button>
            <Button variant="primary" onClick={() => void upload()}>
              {t("common.retry")}
            </Button>
          </>
        ) : (
          <>
            <Button variant="ghost" onClick={close} disabled={state.kind === "uploading"}>
              {t("instance.cancel")}
            </Button>
            <Button
              variant="primary"
              icon={<Globe size={16} />}
              disabled={state.kind === "uploading"}
              onClick={() => void upload()}
            >
              {state.kind === "uploading"
                ? t("instance.logs.share.uploading")
                : t("instance.logs.share.confirm")}
            </Button>
          </>
        )
      }
    >
      {done ? (
        <Input
          readOnly
          value={done.url}
          aria-label={t("instance.logs.share.linkLabel")}
          onFocus={(e) => e.currentTarget.select()}
          data-selectable
          className="font-mono text-[13px]"
        />
      ) : state.kind === "uploading" ? (
        <Progress label={t("instance.logs.share.uploading")} />
      ) : state.kind === "error" ? (
        <div className="flex items-start gap-2.5 rounded-[var(--radius-panel)] bg-flare-400/10 px-3.5 py-3">
          <WarningCircle size={18} className="mt-px shrink-0 text-flare-400" />
          <div className="min-w-0 flex-1">
            <p className="text-[13.5px] text-bone-100">{state.message}</p>
            <button
              type="button"
              onClick={() => void copyErrorDetails(state.error)}
              className="mt-1.5 cursor-pointer text-[12.5px] text-ash-400 underline-offset-2 hover:text-bone-100 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
            >
              {t("common.copyDetails")}
            </button>
          </div>
        </div>
      ) : (
        <ul className="space-y-1.5 text-[13px] text-ash-300">
          <li>{t("instance.logs.share.pointPublic")}</li>
          <li>{t("instance.logs.share.pointMasked")}</li>
        </ul>
      )}
    </Dialog>
  );
}
