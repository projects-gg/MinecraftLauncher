// The question's words (PLAY-ACCT): what starts in the title, what happens to the game that is on
// in one sentence, and the answer's name; then each step on the way (the steps of FIX2 "Bağlan",
// social/connect.ts). Pure: i18n keys with their values (tested in words.test.ts). Names never get
// a suffix: a Turkish case ending would have to follow the name's vowels.
import type { CloseStep } from "@/features/social/connect";
import type { GameAccount } from "@/lib/api";
import type { GameStage } from "@/stores/launch-owner";

export type Line = readonly [key: string, values?: Record<string, string>];

/** The game that is on and what "Oyna" starts instead. */
export interface SwitchSides {
  /** How far the game that is on is. */
  stage: GameStage;
  /** Its account (null: not known) and installation (its name: null until the list is read). */
  from: { account: GameAccount | null; instance: string | null; instanceName: string | null };
  /** The selected account and the installation "Oyna" was pressed for. */
  to: { account: GameAccount | null; instance: string; instanceName: string | null };
  /** A singleplayer world may be open (not on a server, as far as the friends service knows). */
  world: boolean;
}

/** Another account than the one in game: the words name the accounts. */
export function accountChanges(sides: SwitchSides): boolean {
  const from = sides.from.account?.id;
  const to = sides.to.account?.id;
  return Boolean(from && to && from !== to);
}

/** What happens to the game that is on, in one sentence. */
function fate(sides: SwitchSides): Line {
  const { stage, from } = sides;
  const name = from.account?.name ?? "";
  const instance = from.instanceName;
  if (stage === "preparing") {
    return instance
      ? ["playAcct.switch.preparingInstance", { instance }]
      : ["playAcct.switch.preparingGame"];
  }
  const kind = stage === "open" ? "open" : "opening";
  if (accountChanges(sides)) {
    // The installation is named only when it is not the one "Oyna" starts too.
    return instance && from.instance !== sides.to.instance
      ? [`playAcct.switch.${kind}AccountInstance`, { name, instance }]
      : [`playAcct.switch.${kind}Account`, { name }];
  }
  return instance
    ? [`playAcct.switch.${kind}Instance`, { instance }]
    : [`playAcct.switch.${kind}Game`];
}

/** The question: its title, one sentence (and "Dünyan kaydedilir." when it may help), the answer. */
export function questionWords(sides: SwitchSides): { title: Line; body: Line[]; confirm: Line } {
  const to = sides.to;
  const title: Line =
    accountChanges(sides) && to.account
      ? ["playAcct.switch.titleAccount", { name: to.account.name }]
      : to.instanceName
        ? ["playAcct.switch.titleInstance", { instance: to.instanceName }]
        : ["fix2.connect.restartTitle"];
  const open = sides.stage === "open";
  const body: Line[] = [fate(sides)];
  if (open && sides.world) body.push(["fix2.connect.saved"]);
  return {
    title,
    body,
    confirm: [open ? "playAcct.switch.close" : "playAcct.switch.stop"],
  };
}

/** After the answer: where the way is (the game closing, late, being ended). */
export function stepWords(
  sides: SwitchSides,
  step: CloseStep["kind"],
): { title: Line; body: Line[] } {
  const to = sides.to;
  const next: Line =
    accountChanges(sides) && to.account
      ? ["playAcct.switch.closingAccount", { name: to.account.name }]
      : to.instanceName
        ? ["playAcct.switch.closingInstance", { instance: to.instanceName }]
        : ["playAcct.switch.closingGame"];
  switch (step) {
    case "overdue":
      return { title: ["fix2.connect.overdueTitle"], body: [["fix2.connect.overdueBody"]] };
    case "forcing":
      return { title: ["fix2.connect.forcingTitle"], body: [next] };
    case "closing":
      return {
        title: ["fix2.connect.closingTitle"],
        body: sides.world ? [["fix2.connect.saving"], next] : [next],
      };
    default:
      // A start that had not opened yet stops (nothing to save), as quickly as it is said.
      return { title: ["fix2.connect.closingTitle"], body: [next] };
  }
}
