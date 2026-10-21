// Moderators' one-time notes (MOD1): a warning, or a profile picture put back to the default. Each
// shows once, on whichever device sees it first ("Anladım" marks it seen with the service), with the
// reason when the moderators gave one and a link to the community rules.
import { ShieldWarning } from "@phosphor-icons/react";
import { Button, Dialog } from "@pl/ui";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import type { ModNotice } from "@/lib/api/social-types";
import { openWebLink } from "@/lib/links";
import { COMMUNITY_RULES_URL } from "./limits";
import { useSocialUi } from "./store";

export function NoticeDialog() {
  const { t } = useTranslation();
  const notices = useSocialUi((s) => s.view.notices);
  const signedIn = useSocialUi((s) => s.view.auth === "signedIn");
  const [shown, setShown] = useState<ModNotice | null>(null);

  // One at a time, oldest first; the next one comes after "Anladım".
  useEffect(() => {
    if (shown || !signedIn) return;
    const next = notices[0];
    if (next) setShown(next);
  }, [notices, shown, signedIn]);

  const done = () => {
    if (shown) void api.social.noticesSeen([shown.id]);
    setShown(null);
  };

  const warning = shown?.kind === "warning";
  return (
    <Dialog
      open={shown !== null}
      onOpenChange={(open) => {
        if (!open) done();
      }}
      title={
        <span className="flex items-center gap-2.5">
          <ShieldWarning size={20} weight="duotone" className="text-gold-300" aria-hidden />
          {warning ? t("safety.notice.warningTitle") : t("safety.notice.profileResetTitle")}
        </span>
      }
      description={warning ? t("safety.notice.warning") : t("safety.notice.profileReset")}
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
      {shown?.reason ? (
        <p className="text-[13px] text-ash-300">
          {t("safety.notice.reason", { reason: t(`safety.notice.reasons.${shown.reason}`) })}
        </p>
      ) : null}
    </Dialog>
  );
}
