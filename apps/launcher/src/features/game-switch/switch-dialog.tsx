// The question "Oyna" asks while something runs (PLAY-ACCT, PLAY-AGAIN). Another launch on its way
// (one at a time): what starts and what stops, "Durdur ve oyna". One more game (the selected
// account in game: the owner's question with the name; the free memory short: one sentence): one
// shape, "Bir oyun daha açılsın mı?", the reasons in one paragraph, "Yine de aç". The AFK question
// of a same-name account comes with the new launch, as for OYNA. Mounted once (shell/app-shell.tsx).
//
// Second look (PLAY-ACCT): "Vazgeç" keeps one place (the left), nothing comes under the pointer
// that pressed the answer (the short way shows its bar there, at the buttons' height, so the panel
// keeps its size), and a click beside the panel (the rest of a double click on OYNA) does not close
// the question.
import { Play } from "@phosphor-icons/react";
import { Button, Dialog, Progress } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import type { TFunction } from "i18next";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { api } from "@/lib/api";
import { useLaunch } from "@/stores/launch";
import { gameStage } from "@/stores/launch-owner";
import { cancelSwitch, confirmSwitch } from "./flow";
import { type SwitchRequest, useGameSwitch } from "./store";
import { type Line, oneMoreWords, questionWords, type SwitchSides, stepWords } from "./words";

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
  const { data: instances } = useQuery({
    queryKey: ["instances"],
    queryFn: () => api.instances(),
    enabled: request !== null,
  });
  const nameOf = (id: string | null) =>
    id ? (instances?.find((i) => i.id === id)?.name ?? null) : null;
  const live: SwitchSides | null =
    request?.kind === "stop" && stage
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
  // The words stay while the dialog fades out, and when the launch ended before the answer.
  const last = useRef<{
    request: SwitchRequest;
    sides: SwitchSides | null;
    stepping: boolean;
  } | null>(null);
  const sides = frozen ?? live ?? (request ? (last.current?.sides ?? null) : null);
  if (request && (request.kind !== "stop" || sides)) {
    last.current = { request, sides, stepping: current !== null };
  }
  const shown = last.current;
  if (!shown) return null;

  const { kind } = shown.request;
  const words =
    kind === "stop"
      ? shown.stepping && shown.sides
        ? { ...stepWords(shown.sides), confirm: null }
        : shown.sides
          ? questionWords(shown.sides)
          : null
      : oneMoreWords(kind, Boolean(shown.request.memoryShort), shown.request.account?.name ?? null);
  if (!words) return null;
  const title = say(t, words.title);
  // The reasons read as one paragraph (second look: two lines read as two separate notes).
  const body = words.body.map((line) => say(t, line)).join(" ");
  const asking = !shown.stepping;
  const footer =
    asking && words.confirm ? (
      <>
        <Button variant="ghost" className="mr-auto" onClick={cancelSwitch}>
          {t("playAcct.switch.cancel")}
        </Button>
        <Button
          variant="primary"
          icon={<Play size={14} weight="fill" />}
          onClick={() => confirmSwitch(shown.request, shown.sides)}
        >
          {say(t, words.confirm)}
        </Button>
      </>
    ) : (
      // Where the buttons were, at their height: the panel keeps its size while the launch stops.
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
      description={<span className="text-pretty">{body}</span>}
      closeLabel={t("common.close")}
      width={460}
      morph
      dismissible={asking}
      dismissOnOutsideClick={false}
      footer={footer}
    />
  );
}
