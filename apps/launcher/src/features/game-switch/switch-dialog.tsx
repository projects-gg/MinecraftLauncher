// "Oyna" while another game runs (PLAY-ACCT): another account's game (the owner's report: with
// another account selected, OYNA said "Oyunda"), or another installation's, since one game runs
// at a time. One question names what starts and what happens to the game that is on; its answer
// closes that game the way its own close button does (a singleplayer world is saved) and starts
// this one with the selected account (flow.ts). A game still opening, or files being prepared,
// simply stop. The AFK question of a same-name account comes with the new launch, as for OYNA.
// Mounted once (shell/app-shell.tsx).
import { Play } from "@phosphor-icons/react";
import { Button, Dialog } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import type { CloseStep } from "@/features/social/connect";
import { Waiting } from "@/features/social/join-dialog";
import { useSocialUi } from "@/features/social/store";
import { api } from "@/lib/api";
import { useLaunch } from "@/stores/launch";
import { gameStage } from "@/stores/launch-owner";
import { cancelSwitch, confirmSwitch, sendSwitch } from "./flow";
import { type SwitchRequest, useGameSwitch } from "./store";
import { type Line, questionWords, type SwitchSides, stepWords } from "./words";

const say = (t: TFunction, [key, values]: Line) => t(key, values);

export function GameSwitchDialog() {
  const { t } = useTranslation();
  const request = useGameSwitch((s) => s.request);
  const current = useGameSwitch((s) => s.step);
  const frozen = useGameSwitch((s) => s.sides);
  // Primitives only: the dialog stays mounted and must not draw again at every progress step.
  const stage = useLaunch((s) => gameStage(s.phase));
  const fromAccount = useLaunch((s) => s.account);
  const fromInstance = useLaunch((s) => s.instanceId);
  // Not on a server, as far as the friends service knows: a singleplayer world may be open.
  const world = useSocialUi((s) => s.view.activity.state !== "playing");
  const { data: instances } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
    enabled: request !== null,
  });
  const nameOf = (id: string | null) =>
    id ? (instances?.find((i) => i.id === id)?.name ?? null) : null;
  const live: SwitchSides | null =
    request && stage
      ? {
          stage,
          from: {
            account: fromAccount,
            instance: fromInstance,
            instanceName: nameOf(fromInstance),
          },
          to: {
            account: request.account,
            instance: request.instanceId,
            instanceName: nameOf(request.instanceId),
          },
          world,
        }
      : null;
  // The words stay while the dialog fades out, and when the game ended before the answer.
  const last = useRef<{
    request: SwitchRequest;
    sides: SwitchSides;
    step: CloseStep | null;
  } | null>(null);
  const sides = frozen ?? live ?? (request ? last.current?.sides : null);
  if (request && sides) last.current = { request, sides, step: current };
  const shown = last.current;
  if (!shown) return null;

  const step = shown.step?.kind;
  const question = step ? null : questionWords(shown.sides);
  const words = question ?? stepWords(shown.sides, step ?? "closing");
  const title = say(t, words.title);
  const body = words.body.map((line) => say(t, line)).join(" ");
  // An end the player asked for and a start that stops run to their end.
  const settled = step === "forcing" || step === "stopping";
  const cancel = (
    <Button
      variant="ghost"
      className={step === "overdue" ? "mr-auto" : undefined}
      onClick={cancelSwitch}
    >
      {t("fix2.connect.cancel")}
    </Button>
  );
  const footer =
    step === undefined ? (
      <>
        {cancel}
        <Button
          variant="primary"
          icon={<Play size={14} weight="fill" />}
          onClick={() => confirmSwitch(shown.request, shown.sides)}
        >
          {question ? say(t, question.confirm) : null}
        </Button>
      </>
    ) : step === "closing" ? (
      cancel
    ) : step === "overdue" ? (
      <>
        {cancel}
        <Button variant="secondary" onClick={() => sendSwitch({ type: "wait", now: Date.now() })}>
          {t("fix2.connect.wait")}
        </Button>
        <Button variant="danger" onClick={() => sendSwitch({ type: "force", now: Date.now() })}>
          {t("fix2.connect.force")}
        </Button>
      </>
    ) : undefined;
  return (
    <Dialog
      open={request !== null}
      onOpenChange={(open) => {
        if (!open) cancelSwitch();
      }}
      title={title}
      description={body}
      closeLabel={t("common.close")}
      width={460}
      morph
      dismissible={!settled}
      footer={footer}
    >
      {step === undefined || step === "overdue" ? null : <Waiting />}
    </Dialog>
  );
}
