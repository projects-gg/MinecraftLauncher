// The question's words (PLAY-ACCT): what starts in the title, what happens to the game that is on
// in one sentence, and the answer's name; then each step on the way (the steps of FIX2 "Bağlan",
// social/connect.ts, in this question's own words). Pure: i18n keys with their values (tested in
// words.test.ts). Names never get a suffix (a Turkish case ending would have to follow the name's
// vowels), and an installation is called one ("Projects" is also the server).
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
}

/** Another account than the one in game: the words name the accounts. */
export function accountChanges(sides: SwitchSides): boolean {
  const from = sides.from.account?.id;
  const to = sides.to.account?.id;
  return Boolean(from && to && from !== to);
}

const KEY = "playAcct.switch";

/** What happens to the game that is on, in one sentence ("kaydedilip kapanacak" is always true). */
function fate(sides: SwitchSides): Line {
  const { stage, from } = sides;
  const name = from.account?.name ?? "";
  const instance = from.instanceName;
  if (stage === "preparing") {
    return instance ? [`${KEY}.preparingInstance`, { instance }] : [`${KEY}.preparingGame`];
  }
  const kind = stage === "open" ? "open" : "opening";
  if (accountChanges(sides)) {
    // The installation is named only when it is not the one "Oyna" starts too.
    return instance && from.instance !== sides.to.instance
      ? [`${KEY}.${kind}AccountInstance`, { name, instance }]
      : [`${KEY}.${kind}Account`, { name }];
  }
  return instance ? [`${KEY}.${kind}Instance`, { instance }] : [`${KEY}.${kind}Game`];
}

/** The question: its title, one sentence, the answer. */
export function questionWords(sides: SwitchSides): { title: Line; body: Line[]; confirm: Line } {
  const to = sides.to;
  const title: Line =
    accountChanges(sides) && to.account
      ? [`${KEY}.titleAccount`, { name: to.account.name }]
      : to.instanceName
        ? [`${KEY}.titleInstance`, { instance: to.instanceName }]
        : ["fix2.connect.restartTitle"];
  return {
    title,
    body: [fate(sides)],
    confirm: [sides.stage === "open" ? `${KEY}.close` : `${KEY}.stop`],
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
      ? [`${KEY}.closingAccount`, { name: to.account.name }]
      : to.instanceName
        ? [`${KEY}.closingInstance`, { instance: to.instanceName }]
        : [`${KEY}.closingGame`];
  switch (step) {
    case "overdue":
      return { title: [`${KEY}.overdueTitle`], body: [[`${KEY}.overdueBody`]] };
    case "forcing":
      return { title: [`${KEY}.forcingTitle`], body: [next] };
    default:
      // Closing, or a start that had not opened yet stopping (as quickly as it is said).
      return { title: [`${KEY}.closingTitle`], body: [next] };
  }
}
