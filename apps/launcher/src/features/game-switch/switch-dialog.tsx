// "Oyna" while another game runs (PLAY-ACCT): another account's game (the owner's report: with
// another account selected, OYNA said "Oyunda"), or another installation's, since one game runs
// at a time. One question names what starts and what happens to the game that is on; its answer
// saves and closes that game the way its own close button does and starts this one with the
// selected account (flow.ts). A game still opening, or files being prepared, simply stop. The AFK
// question of a same-name account comes with the new launch, as for OYNA. Mounted once
// (shell/app-shell.tsx).
//
// Second look (PLAY-ACCT): "Vazgeç" keeps one place (the left), nothing comes under the pointer
// that pressed "Kapat ve oyna" (the closing step shows its bar there, at the buttons' height, so
// the panel keeps its size), the buttons that come after the 20 s wait take no press for a moment,
// and a click beside the panel (the rest of a double click on OYNA) does not close the question.
import { Play } from "@phosphor-icons/react";
import { Button, type ButtonProps, Dialog, Progress, usePressGuard } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { type MouseEvent, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { CloseStep } from "@/features/social/connect";
import { api } from "@/lib/api";
import { useLaunch } from "@/stores/launch";
import { gameStage } from "@/stores/launch-owner";
import { cancelSwitch, confirmSwitch, sendSwitch } from "./flow";
import { type SwitchRequest, useGameSwitch } from "./store";
import { type Line, questionWords, type SwitchSides, stepWords } from "./words";

const say = (t: TFunction, [key, values]: Line) => t(key, values);

/**
 * A button that came in place of the way's waiting bar: it takes presses once the pointer has left
 * it or a moment after it came (`usePressGuard`), never a click aimed at what was there before.
 */
function ArrivingButton({
  onClick,
  ...props
}: Omit<ButtonProps, "onClick"> & { onClick: () => void }) {
  const [since] = useState(() => performance.now());
  const guard = usePressGuard("arrived", { since });
  return (
    <Button
      {...props}
      aria-disabled={!guard.armed || undefined}
      onPointerLeave={guard.handlers.onPointerLeave}
      onKeyDown={guard.handlers.onKeyDown}
      onClick={(event: MouseEvent<HTMLButtonElement>) => {
        if (guard.take(event)) onClick();
      }}
    />
  );
}

export function GameSwitchDialog() {
  const { t } = useTranslation();
  const request = useGameSwitch((s) => s.request);
  const current = useGameSwitch((s) => s.step);
  const frozen = useGameSwitch((s) => s.sides);
  // Primitives only: the dialog stays mounted and must not draw again at every progress step.
  const stage = useLaunch((s) => gameStage(s.phase));
  const fromAccount = useLaunch((s) => s.account);
  const fromInstance = useLaunch((s) => s.instanceId);
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
  // The question and the late game's choices can be left; the way between runs to its end.
  const open = step === undefined || step === "overdue";
  const footer = question ? (
    <>
      <Button variant="ghost" className="mr-auto" onClick={cancelSwitch}>
        {t("playAcct.switch.cancel")}
      </Button>
      <Button
        variant="primary"
        icon={<Play size={14} weight="fill" />}
        onClick={() => confirmSwitch(shown.request, shown.sides)}
      >
        {say(t, question.confirm)}
      </Button>
    </>
  ) : step === "overdue" ? (
    <>
      <ArrivingButton variant="ghost" className="mr-auto" onClick={cancelSwitch}>
        {t("playAcct.switch.cancel")}
      </ArrivingButton>
      <ArrivingButton
        variant="secondary"
        onClick={() => sendSwitch({ type: "wait", now: Date.now() })}
      >
        {t("playAcct.switch.wait")}
      </ArrivingButton>
      <ArrivingButton
        variant="danger"
        onClick={() => sendSwitch({ type: "force", now: Date.now() })}
      >
        {t("playAcct.switch.force")}
      </ArrivingButton>
    </>
  ) : (
    // Where the buttons were, at their height: the panel keeps its size while the game goes.
    <div className="flex h-[32px] w-full items-center">
      <Progress label={title} className="w-full" />
    </div>
  );
  return (
    <Dialog
      open={request !== null}
      onOpenChange={(next) => {
        if (!next) cancelSwitch();
      }}
      title={title}
      description={body}
      closeLabel={t("common.close")}
      width={460}
      morph
      dismissible={open}
      dismissOnOutsideClick={false}
      footer={footer}
    />
  );
}
