// Moderators' one-time notes (MOD1): a warning, or a profile picture put back to the default; and
// staff in game resetting the Projects skin (SKIN1), with how long skin changes stay off. Each
// shows once, on whichever device sees it first ("Anladım" marks it seen with the service), with the
// reason when the moderators gave one and a link to the community rules.
import { ShieldWarning } from "@phosphor-icons/react";
import { Button, Dialog } from "@pl/ui";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import type { ModNotice } from "@/lib/api/social-types";
import { openWebLink } from "@/lib/links";
import { COMMUNITY_RULES_URL, dayMonth } from "./limits";
import { useSocialUi } from "./store";

export function NoticeDialog() {
  const { t } = useTranslation();
  const notices = useSocialUi((s) => s.view.notices);
  const signedIn = useSocialUi((s) => s.view.auth === "signedIn");
  const [shown, setShown] = useState<ModNotice | null>(null);
  // Closed here: the list keeps a notice until the service took "seen", which must not show it again.
  const dismissed = useRef(new Set<string>());

  // One at a time, oldest first; the next one comes after "Anladım".
  useEffect(() => {
    if (shown || !signedIn) return;
    const next = notices.find((notice) => !dismissed.current.has(notice.id));
    if (next) setShown(next);
  }, [notices, shown, signedIn]);

  const done = () => {
    if (shown) {
      dismissed.current.add(shown.id);
      void api.social.noticesSeen([shown.id]);
    }
    setShown(null);
  };

  const kind = shown?.kind;
  const title =
    kind === "warning"
      ? t("safety.notice.warningTitle")
      : kind === "skinReset"
        ? t("projectsSkin.notice.title")
        : t("safety.notice.profileResetTitle");
  const description =
    kind === "warning"
      ? t("safety.notice.warning")
      : kind === "skinReset"
        ? t("projectsSkin.notice.body")
        : t("safety.notice.profileReset");
  const blocked = kind === "skinReset" ? shown?.blocked : undefined;
  return (
    <Dialog
      open={shown !== null}
      onOpenChange={(open) => {
        if (!open) done();
      }}
      title={
        <span className="flex items-center gap-2.5">
          <ShieldWarning size={20} weight="duotone" className="text-gold-300" aria-hidden />
          {title}
        </span>
      }
      description={description}
      closeLabel={t("common.close")}
      width={440}
      dismissOnOutsideClick={false}
      footer={
        <>
          <Button variant="ghost" onClick={() => openWebLink(COMMUNITY_RULES_URL)}>
            {t("safety.restricted.rules")}
          </Button>
          <Button variant="primary" onClick={done}>
            {t("safety.notice.ok")}
          </Button>
        </>
      }
    >
      {blocked ? (
        <p className="text-[13px] text-bone-100">
          {blocked.until === null
            ? t("projectsSkin.notice.forever")
            : t("projectsSkin.notice.until", { date: dayMonth(blocked.until) })}
        </p>
      ) : null}
      {shown?.reason ? (
        <p className={blocked ? "mt-1.5 text-[13px] text-ash-300" : "text-[13px] text-ash-300"}>
          {t("safety.notice.reason", { reason: t(`safety.notice.reasons.${shown.reason}`) })}
        </p>
      ) : null}
    </Dialog>
  );
}
