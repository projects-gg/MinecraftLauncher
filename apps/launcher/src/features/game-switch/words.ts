// The question's words (PLAY-ACCT, PLAY-AGAIN). "Durdur ve oyna": what starts in the title, what
// happens to the launch on its way in one sentence, the answer's name, then the short way. One
// more game: one shape (second look): the title asks "Bir oyun daha açılsın mı?", the body says
// why (the owner's words with the name, the memory), "Yine de aç" answers. Pure: i18n keys
// with their values (tested in words.test.ts). Names never get a suffix (a Turkish case ending
// would have to follow the name's vowels), and an installation is called one ("Projects" is also
// the server).
import type { GameAccount } from "@/lib/api";
import type { GameStage } from "@/stores/launch-owner";
import type { SwitchKind } from "./store";

export type Line = readonly [key: string, values?: Record<string, string>];

/** The launch on its way and what "Oyna" starts instead. */
export interface SwitchSides {
  /** How far the launch on its way is. */
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

/** What happens to the launch on its way, in one sentence. */
function fate(sides: SwitchSides): Line {
  const { stage, from } = sides;
  const name = from.account?.name ?? "";
  const instance = from.instanceName;
  if (stage === "preparing") {
    return instance ? [`${KEY}.preparingInstance`, { instance }] : [`${KEY}.preparingGame`];
  }
  if (accountChanges(sides)) {
    // The installation is named only when it is not the one "Oyna" starts too.
    return instance && from.instance !== sides.to.instance
      ? [`${KEY}.openingAccountInstance`, { name, instance }]
      : [`${KEY}.openingAccount`, { name }];
  }
  return instance ? [`${KEY}.openingInstance`, { instance }] : [`${KEY}.openingGame`];
}

/** "Durdur ve oyna": its title, one sentence, the answer. */
export function questionWords(sides: SwitchSides): { title: Line; body: Line[]; confirm: Line } {
  const to = sides.to;
  const title: Line =
    accountChanges(sides) && to.account
      ? [`${KEY}.titleAccount`, { name: to.account.name }]
      : to.instanceName
        ? [`${KEY}.titleInstance`, { instance: to.instanceName }]
        : ["fix2.connect.restartTitle"];
  return { title, body: [fate(sides)], confirm: [`${KEY}.stop`] };
}

/** After "Durdur ve oyna": the launch stops (as quickly as it is said), then this one starts. */
export function stepWords(sides: SwitchSides): { title: Line; body: Line[] } {
  const to = sides.to;
  const next: Line =
    accountChanges(sides) && to.account
      ? [`${KEY}.closingAccount`, { name: to.account.name }]
      : to.instanceName
        ? [`${KEY}.closingInstance`, { instance: to.instanceName }]
        : [`${KEY}.closingGame`];
  return { title: [`${KEY}.closingTitle`], body: [next] };
}

const AGAIN = "playAgain";

/**
 * One more game (PLAY-AGAIN), one shape whatever the reason: the title asks, the body says why
 * (the selected account `name` in game already: the owner's words with it; the memory short: one
 * sentence after it), "Yine de aç" answers. Another account's games only: asked only when the
 * memory is short.
 */
export function oneMoreWords(
  kind: Exclude<SwitchKind, "stop">,
  memoryShort: boolean,
  name: string | null = null,
): { title: Line; body: Line[]; confirm: Line } {
  const same: Line[] =
    kind === "again" ? [name ? [`${AGAIN}.sameName`, { name }] : [`${AGAIN}.sameNameUnknown`]] : [];
  const memory: Line[] = memoryShort ? [[`${AGAIN}.memory`]] : [];
  return { title: [`${AGAIN}.title`], body: [...same, ...memory], confirm: [`${AGAIN}.confirm`] };
}
