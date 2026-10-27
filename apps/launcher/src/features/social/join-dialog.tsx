// "Katıl" on a "Birlikte oyna" invite (its toast or its row in the list), "Geliyorum" to a crew call
// on a server (joinCall), and every other "Katıl" while a game is open (joinOrAsk). With the game
// closed: "Minecraft açılsın mı?" and "Bağlan" starts the game straight into the server
// (projects.gg as every "Katıl" does; another server with the instance OYNA would start).
// With a game open (FIX2 "Bağlan"): one question; then the overlay takes the player there in game,
// or the game closes the way its own close button does (it leaves its server, a singleplayer world
// is saved) and starts again into the server. A game that does not close by itself is ended only at
// the player's word, after one sentence about what may be lost. While a game is prepared or opens,
// its start stops and begins again into the server. The address sits under the question with its
// copy button, for whoever would rather join from the game. Mounted once, by SocialHost.
// A friend's world (HOST1, joinWorld) goes the same way once its join is ready, in its own words
// and without an address: the game moves in game only when it runs the install the join prepared.
// With several games (PLAY-AGAIN) "the game" is the selected account's newest one on screen; with
// none of its own, a new game starts into the server beside the others.
//
// The buttons keep their places (PLAY-AGAIN, from PLAY-ACCT's second look): "Vazgeç" is on the
// left at every step, so nothing comes under the pointer that pressed "Bağlan"; the buttons that
// come after the 20 s wait take no press for a moment.
import { Check, Copy, Play } from "@phosphor-icons/react";
import { Button, type ButtonProps, Dialog, IconButton, toast, usePressGuard } from "@pl/ui";
import type { TFunction } from "i18next";
import { AnimatePresence, motion } from "motion/react";
import { type MouseEvent, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { announce } from "@/features/accounts/use-accounts";
import { startFriendWorld } from "@/features/host/launch";
import { api, type GameAccount } from "@/lib/api";
import type { SocialInvite } from "@/lib/api/social-types";
import i18n from "@/lib/i18n";
import { useLaunch } from "@/stores/launch";
import { type OwnLaunch, ownLaunch } from "@/stores/launch-owner";
import { copyText } from "./actions";
import {
  CLOSE_GRACE_MS,
  type CloseEvent,
  type CloseStep,
  closeNext,
  FORCE_GRACE_MS,
  gameNow,
  IN_GAME_LIMIT_MS,
} from "./connect";
import { connectInGame, inGameReady } from "./connect-in-game";
import type { CallWhere } from "./crews";
import { joinProjects, joinServer } from "./join";
import { pop, useSocialMotion } from "./motion";
import {
  type JoinTarget,
  joinStep,
  joinTarget,
  onTarget,
  restartInstance,
  targetAddress,
  targetName,
} from "./play-together";
import { useSocialUi } from "./store";

interface Ask {
  /** The invite answered by joining (it leaves the list on "Bağlan"); none for a crew call. */
  inviteId?: string;
  target: JoinTarget;
  step: "confirm" | "restart" | "reopen";
  /** The overlay is in the running game: it may take the player there without a restart. */
  inGame: boolean;
  /** A singleplayer world may be open (not on a server, as far as the friends service knows). */
  world: boolean;
  /** After "Bağlan" with a game open: where it is (connect.ts). */
  closing?: CloseStep;
}

const useJoinAsk = create<{ ask: Ask | null }>()(() => ({ ask: null }));

/** The way after "Bağlan", one at a time: a new question or "Vazgeç" leaves it behind. */
interface Flow {
  id: number;
  target: JoinTarget;
  /** The instance the game ran (it starts again where it can go). */
  running: string | null;
  /** The account it played with: it starts again with it, whichever is selected (PLAY-ACCT). */
  account: GameAccount | null;
  /** The launcher's number for the game that closes (PLAY-AGAIN: one among several). */
  game: number | null;
  /** The step's own timer (its time limit). */
  timer?: number;
  unsubscribe: () => void;
}

/** The selected account's launch or newest game on screen (PLAY-AGAIN): what "Bağlan" moves. */
function own(): OwnLaunch {
  const launch = useLaunch.getState();
  return ownLaunch(launch, launch.selected);
}

let flow: Flow | null = null;
let flows = 0;

function leave() {
  if (!flow) return;
  window.clearTimeout(flow.timer);
  flow.unsubscribe();
  flow = null;
}

function closeAsk() {
  leave();
  useJoinAsk.setState({ ask: null });
}

/** Opens the question for `target` as the game is now; false when the player is there already. */
function ask(target: JoinTarget, inviteId?: string): boolean {
  const { phase, instanceId } = own();
  const activity = useSocialUi.getState().view.activity;
  const step = joinStep(target, gameNow(phase), activity);
  if (step === "already") return false;
  leave();
  // A friend's world is reached in game only from the install its join prepared.
  const sameInstance = target.kind !== "world" || instanceId === target.instance;
  useJoinAsk.setState({
    ask: {
      inviteId,
      target,
      step,
      inGame: step === "restart" && sameInstance && inGameReady(),
      world: activity.state !== "playing",
    },
  });
  return true;
}

/**
 * A friend's world once its join is ready (HOST1): with the game closed it starts straight in (no
 * question: "Katıl" said it); with a game open or opening, the question first.
 */
export function joinWorld(target: Extract<JoinTarget, { kind: "world" }>) {
  if (gameNow(own().phase) === "closed") {
    void start(target, null);
    return;
  }
  ask(target);
}

/** "Katıl" on an invite: asks first (and says so when the player is there already). */
export function joinInvite(invite: SocialInvite) {
  const target = joinTarget(invite.server);
  if (!target) return;
  if (ask(target, invite.id)) return;
  toast(i18n.t("playTogether.already"));
  void api.social.dismissInvite(invite.id);
}

/**
 * "Geliyorum" to a crew call, or its "Katıl": the same question as an invite's (the game opens
 * straight into the caller's server, projects.gg or another one). Already there, the answer says
 * it all.
 */
export function joinCall(where: CallWhere = { kind: "projects" }) {
  const target: JoinTarget | null =
    where.kind === "projects"
      ? { kind: "projects" }
      : where.kind === "server"
        ? joinTarget({ where: "other", host: where.host })
        : null;
  if (target) ask(target);
}

/**
 * Every other "Katıl" (a friend on projects.gg, a crew's plan, Home's lines): with the game closed
 * it starts at once, as it always did; with a game open or opening, the question first (FIX2).
 */
export function joinOrAsk(target: JoinTarget) {
  if (gameNow(own().phase) === "closed") {
    void start(target, null);
    return;
  }
  if (!ask(target)) toast(i18n.t("playTogether.already"));
}

/** projects.gg, where most "Katıl" lead. */
export const PROJECTS: JoinTarget = { kind: "projects" };

/** The player's game is on `target` already: its "Katıl" is off. */
export function useThere(target: JoinTarget): boolean {
  const open = useLaunch((s) => gameNow(ownLaunch(s, s.selected).phase) === "open");
  const activity = useSocialUi((s) => s.view.activity);
  return open && onTarget(target, activity);
}

/** Whether an invite leads somewhere "Katıl" can go. */
export function canJoinInvite(invite: SocialInvite): boolean {
  return joinTarget(invite.server) !== null;
}

/**
 * Starts the game into `target`: with the instance that ran, where it can go (`running`), and the
 * account it played with (`account`; none: the selected one).
 */
async function start(
  target: JoinTarget,
  running: string | null,
  account: GameAccount | null = null,
) {
  const as = account ?? undefined;
  if (target.kind === "world") {
    startFriendWorld(target, as);
    return;
  }
  const instances = running ? await api.instances().catch(() => []) : [];
  const prefer = restartInstance(target, running, instances) ?? undefined;
  if (target.kind === "projects") await joinProjects(prefer, as);
  else await joinServer(target, prefer, as);
}

/** One step on the way after "Bağlan": the answers, the timers and the game's end lead here. */
function step(id: number, event: CloseEvent) {
  const current = useJoinAsk.getState().ask;
  if (!flow || flow.id !== id || !current?.closing) return;
  const next = closeNext(current.closing, event);
  if (next === current.closing) return;
  const { target, running, account } = flow;
  if (next === "connected" || next === "start") {
    closeAsk();
    announce(
      target.kind === "world"
        ? i18n.t("host.connect.connecting")
        : i18n.t("fix2.connect.connecting", { server: targetName(target) }),
    );
    if (next === "start") void start(target, running, account);
    return;
  }
  useJoinAsk.setState({ ask: { ...current, closing: next } });
  begin(flow, next);
}

/** Each step's title (the dialog's, and what a screen reader hears as it begins). */
const STEP_TITLES: Record<CloseStep["kind"], string> = {
  inGame: "fix2.connect.movingTitle",
  closing: "fix2.connect.closingTitle",
  stopping: "fix2.connect.closingTitle",
  overdue: "fix2.connect.overdueTitle",
  forcing: "fix2.connect.forcingTitle",
};

/** What a step does as it begins; its time limit, if it has one. */
function begin(current: Flow, next: CloseStep) {
  const { id } = current;
  const limit = (ms: number, event: () => CloseEvent) => {
    window.clearTimeout(current.timer);
    current.timer = window.setTimeout(() => step(id, event()), ms);
  };
  // The dialog's title changed in place: a screen reader hears where it is now.
  announce(i18n.t(STEP_TITLES[next.kind]));
  switch (next.kind) {
    case "inGame":
      // The shell answers within five seconds; past the limit it counts as a no.
      limit(IN_GAME_LIMIT_MS, () => ({ type: "inGame", done: false, now: Date.now() }));
      void connectInGame(current.target, current.game).then((done) =>
        step(id, { type: "inGame", done, now: Date.now() }),
      );
      break;
    case "closing": {
      limit(CLOSE_GRACE_MS, () => ({ type: "tick", now: Date.now() }));
      const { game } = current;
      // A game the page cannot name cannot be asked: the warning comes at once.
      const asking = game === null ? Promise.resolve(false) : api.closeGame(game);
      void asking.catch(() => false).then((asked) => step(id, { type: "asked", asked }));
      break;
    }
    case "forcing":
      limit(FORCE_GRACE_MS, () => ({ type: "tick", now: Date.now() }));
      if (current.game !== null) void api.killGame(current.game);
      break;
    case "stopping":
      // Nothing to save yet: the start stops (a game that was opening closes with it).
      void useLaunch.getState().cancel();
      break;
    case "overdue":
      window.clearTimeout(current.timer);
      break;
  }
}

/** "Bağlan": straight in with the game closed; else the way above, from its first step. */
function connect(current: Ask) {
  if (current.inviteId) void api.social.dismissInvite(current.inviteId);
  const launch = useLaunch.getState();
  const mine = own();
  const game = gameNow(mine.phase);
  if (game === "closed") {
    closeAsk();
    void start(current.target, null);
    return;
  }
  leave();
  flows += 1;
  const id = flows;
  // The launch on its way, or this account's newest game on screen (PLAY-AGAIN: other games may
  // run beside it and stay).
  const run = mine.game?.run ?? launch.progress;
  // Its end, however it comes (asked, ended, or closed by the player meanwhile).
  const unsubscribe = useLaunch.subscribe((state) => {
    if (state.progress !== run && !state.games.some((g) => g.run === run)) {
      step(id, { type: "exited" });
    }
  });
  flow = {
    id,
    target: current.target,
    running: mine.instanceId,
    account: mine.game?.account ?? launch.account,
    game: mine.game?.game ?? null,
    unsubscribe,
  };
  const first: CloseStep =
    game === "opening"
      ? { kind: "stopping" }
      : current.inGame
        ? { kind: "inGame" }
        : { kind: "closing", since: Date.now() };
  useJoinAsk.setState({ ask: { ...current, closing: first } });
  begin(flow, first);
}

/** The step's buttons send it on. */
function send(event: CloseEvent) {
  if (flow) step(flow.id, event);
}

/** The server's address with its copy button. */
function AddressRow({ target }: { target: JoinTarget }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = window.setTimeout(() => setCopied(false), 1600);
    return () => window.clearTimeout(timer);
  }, [copied]);
  const address = targetAddress(target);
  const copy = () =>
    void copyText(address).then((ok) => {
      if (!ok) return;
      setCopied(true);
      announce(t("playTogether.addressCopied"));
    });
  return (
    <div className="mt-1">
      {/* What the address is for (second look: the field said nothing of it). */}
      <p className="mb-1.5 text-[12.5px] text-ash-400">{t("playAgain.addressHint")}</p>
      <div className="flex items-center gap-2 rounded-[var(--radius-panel)] bg-obsidian-950/45 py-1 pr-1 pl-3 ring-1 ring-white/8">
        <span className="min-w-0 flex-1 truncate text-[13px] font-medium text-bone-100 select-text">
          <span className="sr-only">{t("fix2.connect.address")}: </span>
          {address}
        </span>
        <IconButton
          size="sm"
          label={copied ? t("playTogether.addressCopied") : t("playTogether.copyAddress")}
          onClick={copy}
          icon={
            <span className="relative flex size-4 items-center justify-center">
              <AnimatePresence initial={false} mode="popLayout">
                <motion.span key={copied ? "done" : "copy"} className="flex" {...pop(reduce, 0.16)}>
                  {copied ? (
                    <Check size={15} weight="bold" className="text-jade-400" />
                  ) : (
                    <Copy size={15} />
                  )}
                </motion.span>
              </AnimatePresence>
            </span>
          }
        />
      </div>
    </div>
  );
}

/**
 * A quiet line that fills from the left over the time the dialog waits for this step (the game
 * closing, a forced end, a move in game), once: full when the wait would run out (second look:
 * the endless sweep read as a stray speck in a still). None without movement, or for a start that
 * stops (no wait of its own).
 */
function Waiting({ ms }: { ms: number | null }) {
  const { reduce } = useSocialMotion();
  if (reduce || ms === null) return null;
  return (
    <div aria-hidden className="relative mt-2 h-[3px] overflow-hidden rounded-full bg-white/10">
      <motion.span
        className="absolute inset-0 origin-left rounded-full bg-gold-400/70"
        initial={{ transform: "scaleX(0)" }}
        animate={{ transform: "scaleX(1)" }}
        transition={{ duration: ms / 1000, ease: "linear" }}
      />
    </div>
  );
}

/** How long the dialog waits at a step before it says more (connect.ts). */
function waitOf(closing: CloseStep["kind"]): number | null {
  if (closing === "closing") return CLOSE_GRACE_MS;
  if (closing === "forcing") return FORCE_GRACE_MS;
  if (closing === "inGame") return IN_GAME_LIMIT_MS;
  return null;
}

/**
 * Title and sentence for where the question or the way is. `closingName`: the account of the
 * game that would close, named when several games are on screen (PLAY-AGAIN, second look).
 */
function words(t: TFunction, shown: Ask, closingName: string | null) {
  if (shown.target.kind === "world") return worldWords(t, shown, closingName);
  const server = targetName(shown.target);
  const saved = shown.world ? ` ${t("fix2.connect.saved")}` : "";
  const closing = shown.closing?.kind;
  if (closing === "inGame") {
    return { title: t("fix2.connect.movingTitle"), body: t("fix2.connect.movingBody", { server }) };
  }
  if (closing === "closing" || closing === "stopping") {
    const saving = shown.world && closing === "closing" ? `${t("fix2.connect.saving")} ` : "";
    return {
      title: t("fix2.connect.closingTitle"),
      body: saving + t("fix2.connect.closingBody", { server }),
    };
  }
  if (closing === "overdue") {
    return { title: t("fix2.connect.overdueTitle"), body: t("fix2.connect.overdueBody") };
  }
  if (closing === "forcing") {
    return {
      title: t("fix2.connect.forcingTitle"),
      body: t("fix2.connect.closingBody", { server }),
    };
  }
  if (shown.step === "confirm") {
    return {
      title: t("playTogether.confirmTitle"),
      body: t("playTogether.confirmBody", { server }),
    };
  }
  if (shown.step === "reopen") {
    return {
      title: t("fix2.connect.restartTitle"),
      body: t("fix2.connect.reopenBody", { server }),
    };
  }
  return shown.inGame
    ? { title: t("fix2.connect.moveTitle"), body: t("fix2.connect.moveBody", { server }) + saved }
    : {
        title: t("fix2.connect.restartTitle"),
        body:
          (closingName
            ? t("playAgain.restartBodyAccount", { name: closingName, server })
            : t("fix2.connect.restartBody", { server })) + saved,
      };
}

/** The same steps for a friend's world (HOST1): "arkadaşının dünyası", never an address. */
function worldWords(t: TFunction, shown: Ask, closingName: string | null) {
  const saved = shown.world ? ` ${t("fix2.connect.saved")}` : "";
  const closing = shown.closing?.kind;
  if (closing === "inGame") {
    return { title: t("fix2.connect.movingTitle"), body: t("host.connect.movingBody") };
  }
  if (closing === "closing" || closing === "stopping") {
    const saving = shown.world && closing === "closing" ? `${t("fix2.connect.saving")} ` : "";
    return {
      title: t("fix2.connect.closingTitle"),
      body: saving + t("host.connect.closingBody"),
    };
  }
  if (closing === "overdue") {
    return { title: t("fix2.connect.overdueTitle"), body: t("fix2.connect.overdueBody") };
  }
  if (closing === "forcing") {
    return { title: t("fix2.connect.forcingTitle"), body: t("host.connect.closingBody") };
  }
  if (shown.step === "reopen") {
    return { title: t("fix2.connect.restartTitle"), body: t("host.connect.reopenBody") };
  }
  return shown.inGame
    ? { title: t("host.connect.moveTitle"), body: t("host.connect.moveBody") + saved }
    : {
        title: t("fix2.connect.restartTitle"),
        body:
          (closingName
            ? t("playAgain.restartWorldAccount", { name: closingName })
            : t("host.connect.restartBody")) + saved,
      };
}

/**
 * A button that came in place of the waiting line (the 20 s wait ran out): it takes presses once
 * the pointer has left it or a moment after it came (`usePressGuard`), never a click aimed at what
 * was there before.
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

export function JoinDialog() {
  const { t } = useTranslation();
  const current = useJoinAsk((s) => s.ask);
  // Several games on screen: the sentence names whose game closes (the selected account's).
  const gameCount = useLaunch((s) => s.games.length);
  // The dialog keeps its words while it fades out.
  const last = useRef<Ask | null>(null);
  if (current) last.current = current;
  const shown = current ?? last.current;
  if (!shown) return null;
  const closingName = gameCount > 1 ? (own().game?.account?.name ?? null) : null;
  const { title, body } = words(t, shown, closingName);
  const closing = shown.closing?.kind;
  // A move in game, a stopping start and an end the player asked for run to their end.
  const settled = closing === "inGame" || closing === "forcing" || closing === "stopping";
  // "Vazgeç" stays on the left: after "Bağlan" nothing comes where the pointer pressed.
  const cancel = (
    <Button variant="ghost" className="mr-auto" onClick={closeAsk}>
      {t("fix2.connect.cancel")}
    </Button>
  );
  const footer =
    closing === undefined ? (
      <>
        {cancel}
        <Button
          variant="primary"
          icon={<Play size={14} weight="fill" />}
          onClick={() => connect(shown)}
        >
          {t("fix2.connect.connect")}
        </Button>
      </>
    ) : closing === "closing" ? (
      cancel
    ) : closing === "overdue" ? (
      <>
        {/* What it does, by name (second look: "Vazgeç" read like "Beklemeye devam et"). */}
        <ArrivingButton variant="ghost" className="mr-auto" onClick={closeAsk}>
          {t("playAgain.keepOpen")}
        </ArrivingButton>
        <ArrivingButton variant="secondary" onClick={() => send({ type: "wait", now: Date.now() })}>
          {t("fix2.connect.wait")}
        </ArrivingButton>
        <ArrivingButton variant="danger" onClick={() => send({ type: "force", now: Date.now() })}>
          {t("fix2.connect.force")}
        </ArrivingButton>
      </>
    ) : undefined;
  return (
    <Dialog
      open={current !== null}
      onOpenChange={(open) => {
        if (!open) closeAsk();
      }}
      title={title}
      description={body}
      closeLabel={t("common.close")}
      width={460}
      morph
      dismissible={!settled}
      footer={footer}
    >
      {closing === undefined ? (
        // A friend's world has no address to show (host1.md §1.1).
        shown.target.kind === "world" ? null : (
          <AddressRow target={shown.target} />
        )
      ) : closing === "overdue" ? null : (
        <Waiting key={closing} ms={waitOf(closing)} />
      )}
    </Dialog>
  );
}
