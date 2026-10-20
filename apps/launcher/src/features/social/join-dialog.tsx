// "Katıl" on a "Birlikte oyna" invite (its toast or its row in the list). With the game closed:
// one question, "Minecraft açılsın mı?", and "Evet" starts the game straight into the inviter's
// server (projects.gg as every "Katıl" does; another server with the instance OYNA would start).
// With a game open or opening it is never closed for this (unsaved singleplayer progress would be
// lost, and the launcher cannot ask the game to save): the dialog says to join from the game's
// Multiplayer screen and offers the address to copy. Mounted once, by SocialHost.
import { Button, Dialog, toast } from "@pl/ui";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { SocialInvite } from "@/lib/api/social-types";
import i18n from "@/lib/i18n";
import { useLaunch } from "@/stores/launch";
import { copyText } from "./actions";
import { gameBusy, joinProjects, joinServer } from "./join";
import { type JoinTarget, joinStep, joinTarget, targetAddress, targetName } from "./play-together";
import { useSocialUi } from "./store";

interface Ask {
  invite: SocialInvite;
  target: JoinTarget;
  step: "confirm" | "gameOpen";
}

const useJoinAsk = create<{ ask: Ask | null }>()(() => ({ ask: null }));

/** "Katıl" on an invite: asks first, or says how to join from the open game. */
export function joinInvite(invite: SocialInvite) {
  const target = joinTarget(invite.server);
  if (!target) return;
  const busy = gameBusy(useLaunch.getState().phase.kind);
  const step = joinStep(target, busy, useSocialUi.getState().view.activity);
  if (step === "already") {
    toast(i18n.t("playTogether.already"));
    void api.social.dismissInvite(invite.id);
    return;
  }
  useJoinAsk.setState({ ask: { invite, target, step } });
}

/** Whether an invite leads somewhere "Katıl" can go. */
export function canJoinInvite(invite: SocialInvite): boolean {
  return joinTarget(invite.server) !== null;
}

export function JoinDialog() {
  const { t } = useTranslation();
  const ask = useJoinAsk((s) => s.ask);
  // The dialog keeps its words while it fades out.
  const last = useRef<Ask | null>(null);
  if (ask) last.current = ask;
  const shown = ask ?? last.current;
  const close = () => useJoinAsk.setState({ ask: null });
  if (!shown) return null;
  const server = targetName(shown.target);

  const yes = () => {
    close();
    void api.social.dismissInvite(shown.invite.id);
    const target = shown.target;
    if (target.kind === "projects") void joinProjects();
    else void joinServer(target);
  };

  const copy = () =>
    void copyText(targetAddress(shown.target)).then((ok) => {
      if (!ok) return;
      toast.success(t("playTogether.addressCopied"));
      announce(t("playTogether.addressCopied"));
    });

  const confirm = shown.step === "confirm";
  return (
    <Dialog
      open={ask !== null}
      onOpenChange={(open) => {
        if (!open) close();
      }}
      title={confirm ? t("playTogether.confirmTitle") : t("playTogether.openTitle")}
      description={
        confirm ? t("playTogether.confirmBody", { server }) : t("playTogether.openBody", { server })
      }
      closeLabel={t("common.close")}
      width={460}
      footer={
        confirm ? (
          <>
            <Button variant="ghost" onClick={close}>
              {t("playTogether.no")}
            </Button>
            <Button variant="primary" onClick={yes}>
              {t("playTogether.yes")}
            </Button>
          </>
        ) : (
          <>
            <Button variant="ghost" onClick={copy}>
              {t("playTogether.copyAddress")}
            </Button>
            <Button variant="primary" onClick={close}>
              {t("playTogether.ok")}
            </Button>
          </>
        )
      }
    />
  );
}
