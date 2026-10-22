// The crew page of the friends panel. With several crews (TEAM4) a rail of their marks sits on top
// (crew-rail.tsx: the player's order, the crew on screen, "Ekip kur ya da katıl" while one more
// fits); the page below shows one crew, sliding in from the side its mark sits on. Without a crew
// (or on "Ekip kur ya da katıl"): invites to crews, "Ekip kur" (crew-editor: name, tag, motto and
// look, with a live preview) and "Kodla katıl" (a code, then what it leads to); at the crew limit,
// one line instead. In a crew: its card (the mark, name, tag, motto, size and founding day, a
// month's or a year's anniversary that day, the emblem's faint watermark) whose strip shows who is
// online with "Ekibi çağır" (this crew; "Birden çok ekibi çağır" beside it with several), or the
// call that is on; where a call cannot start yet (CALL1) one line under the button says why. Then
// the members grouped by where they are ("Projects'te 4", with "Katıl"); friends to invite; the
// crew code; the leader's "Yetkiler"; leaving. Every irreversible step asks once, in place, with
// one sentence; nothing else asks. With chat (CHAT1) the crew has "Ekip" and "Sohbet" (its chat,
// filling the panel); a crew whose name moderators reset says so on top while it stays locked
// (MOD1), and a new Discord account hears in place why it cannot found a crew yet (SPAM1). The
// second wave (TEAM5): join requests above the members for those who answer them
// (crew-requests.tsx), "Planlar" (crew-plans-view.tsx), the invite link and "Katılım" under the
// code (crew-link.tsx), the time played together on the card; "Kodla katıl" takes invite links
// too (pasted, or `projects://crew/…`), and a crew whose leaders approve newcomers gets a join
// request instead ("İsteğin bekliyor", "Geri çek"); the player's own waiting requests sit on top
// of "Ekip kur ya da katıl".
import {
  ArrowsClockwise,
  CalendarBlank,
  CaretDown,
  Check,
  Clock,
  Copy,
  LockSimple,
  MagnifyingGlass,
  MegaphoneSimple,
  PencilSimple,
  ShieldWarning,
  SignOut,
  Sparkle,
  Trash,
  UserPlus,
  UsersThree,
  WarningCircle,
} from "@phosphor-icons/react";
import {
  Button,
  cn,
  IconButton,
  SearchInput,
  Skeleton,
  Switch,
  TabPanel,
  Tabs,
  Tooltip,
} from "@pl/ui";
import { AnimatePresence, LayoutGroup, motion } from "motion/react";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce, friendlyError } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type {
  Friend,
  SocialTeam,
  TeamChange,
  TeamLookup,
  TeamMember,
  TeamPermissions,
} from "@/lib/api/social-types";
import { dateTimeFormat } from "@/lib/format";
import { matchesQuery } from "@/lib/search";
import { attempt, copyText } from "./actions";
import { callNames, callWaitUntil, liveCalls, waitMinutes } from "./calls";
import { CrewChat } from "./chat/crew-chat";
import { CallStrip } from "./crew-call";
import { CallReason, useCallPress } from "./crew-call-picker";
import {
  type CrewDraft,
  CrewEditor,
  CrewPreview,
  draftOf,
  draftReady,
  fieldClass,
  NEW_CREW,
} from "./crew-editor";
import { type CrewKey, crewKeyOf, formatJoinInput, joinOffer, showsLink } from "./crew-joining";
import { CrewLinkRows } from "./crew-link";
import { CrewPlans } from "./crew-plans-view";
import { CrewRail } from "./crew-rail";
import { CrewRequests, MyRequests } from "./crew-requests";
import { callableCrews, canAddCrew, NEW_CREW_TAB, shownCrew } from "./crews";
import { dayMonth, limitOf, opensIn } from "./limits";
import { openMenuFromKey } from "./menu-key";
import { type Leaving, leavingOf } from "./menus";
import { enter, glide, leave, rise, swap, useSocialMotion } from "./motion";
import { PersonContextMenu } from "./person-menu";
import { SocialAvatar, SocialFace } from "./social-face";
import { type CrewPane, useSocialUi } from "./store";
import { CrewMark, CrewWatermark, TeamTag } from "./team-emblem";
import { RoleBadge, TeamMemberRow } from "./team-member";
import { TeamInviteRow } from "./team-section";
import {
  cleanCrewText,
  crewAnniversary,
  crewRights,
  formatTeamCodeInput,
  localDay,
  membersMayHave,
  RIGHTS,
  type Right,
  rosterGroups,
  switchesOf,
  switchLevel,
} from "./teams";
import { togetherTime } from "./together";
import { useCallClock } from "./use-call-clock";

function SectionTitle({ children }: { children: string }) {
  return <h3 className="mb-2.5 text-[12.5px] font-medium text-ash-400">{children}</h3>;
}

function useCrewFeatures() {
  return useSocialUi((s) => s.view.crew);
}

function CreateCrew() {
  const { t } = useTranslation();
  const crew = useCrewFeatures();
  const me = useSocialUi((s) => s.view.me);
  const [draft, setDraft] = useState<CrewDraft>(NEW_CREW);
  const [touched, setTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  // A new Discord account cannot found a crew yet (SPAM1): said in place of the form, so nobody
  // fills it in for nothing (joining by code or invite stays open).
  const limit = limitOf(me, "crewFound", Date.now());

  const submit = async () => {
    setTouched(true);
    if (busy || !draftReady(draft, crew.look, true)) return;
    setBusy(true);
    const name = cleanCrewText(draft.name);
    const done = await attempt(() =>
      api.social.createTeam({
        name,
        tag: draft.tag,
        emblem: draft.emblem,
        ...(crew.look
          ? { color: draft.color, frame: draft.frame, motto: cleanCrewText(draft.motto) }
          : {}),
      }),
    );
    setBusy(false);
    if (done !== undefined) announce(t("teams.create.done", { name }));
  };

  if (limit) {
    return (
      <section className="rounded-[var(--radius-panel)] bg-obsidian-950/40 p-4 ring-1 ring-white/8">
        <SectionTitle>{t("teams.create.title")}</SectionTitle>
        <p className="flex items-start gap-2.5 text-[13px] leading-snug text-ash-300">
          <LockSimple size={16} className="mt-px shrink-0 text-ash-400" aria-hidden />
          {t("safety.limited.crewFound", { when: opensIn(t, limit.until, Date.now()) })}
        </p>
      </section>
    );
  }

  return (
    <section className="rounded-[var(--radius-panel)] bg-obsidian-950/40 p-4 ring-1 ring-white/8">
      <SectionTitle>{t("teams.create.title")}</SectionTitle>
      <CrewEditor
        draft={draft}
        onChange={setDraft}
        look={crew.look}
        texts
        meta={t("teams.members", { count: 1 })}
        showErrors={touched}
        onSubmit={() => void submit()}
        footer={
          <Button type="submit" variant="primary" className="w-full" disabled={busy}>
            {t("teams.create.submit")}
          </Button>
        }
      />
    </section>
  );
}

/** Moderators reset the crew's name and motto: said on top while they stay locked. */
function CrewResetNote({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const until = team.textLockedUntil;
  if (!until || until * 1000 <= Date.now()) return null;
  return (
    <p className="mb-3 flex items-start gap-2.5 rounded-[var(--radius-panel)] bg-gold-500/[0.07] px-3.5 py-3 text-[13px] leading-snug text-bone-100 ring-1 ring-gold-400/20">
      <ShieldWarning
        size={17}
        weight="duotone"
        className="mt-px shrink-0 text-gold-300"
        aria-hidden
      />
      <span>
        {t("safety.crewReset.body")} {t("safety.crewReset.until", { date: dayMonth(until) })}
      </span>
    </p>
  );
}

type Found =
  | { kind: "idle" }
  | { kind: "looking" }
  | { kind: "found"; lookup: TeamLookup }
  | { kind: "error"; message: string };

/** "Kodla katıl" (with links: "Kodla ya da bağlantıyla katıl"); `first` on top of the page (an
 * invite link opened it), without the space above. */
function JoinCrew({ first = false }: { first?: boolean }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const limit = useSocialUi((s) => s.view.crewLimit);
  const features = useCrewFeatures();
  const invites = useSocialUi((s) => s.view.teamInvites);
  const ownRequests = useSocialUi((s) => s.view.teamRequests.length);
  const prefill = useSocialUi((s) => s.prefillLink);
  const inputId = useId();
  const [value, setValue] = useState("");
  const [invalid, setInvalid] = useState(false);
  const [found, setFound] = useState<Found>({ kind: "idle" });
  const [busy, setBusy] = useState(false);
  const pasted = useRef(false);
  // Links are taken where the service has them; the words follow.
  const words = features.link ? "crewWave2.join" : "teams.join";

  const lookUp = (key: CrewKey) =>
    key.kind === "link" ? api.social.lookupCrewLink(key.link) : api.social.lookupTeam(key.code);

  const look = async (input: string) => {
    const key = crewKeyOf(input);
    if (!key) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    setFound({ kind: "looking" });
    try {
      setFound({ kind: "found", lookup: await lookUp(key) });
    } catch (error) {
      setFound({
        kind: "error",
        message: friendlyError(error, t("social.actions.failed")).message,
      });
    }
  };

  // An invite link the player opened (`projects://crew/…`): shown and looked up once; nothing is
  // sent until they press the button.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `look` is this render's
  useEffect(() => {
    if (!prefill) return;
    const link = useSocialUi.getState().takeLink();
    if (!link) return;
    setValue(link);
    void look(link);
  }, [prefill]);

  const join = async (lookup: TeamLookup) => {
    setBusy(true);
    const result = await attempt(() =>
      lookup.link ? api.social.joinCrewLink(lookup.link) : api.social.joinTeam(lookup.code),
    );
    setBusy(false);
    if (result === undefined) return;
    if (result === "requested") {
      announce(t("crewWave2.join.sent"));
      setFound({
        kind: "found",
        lookup: { ...lookup, requestedAt: Math.floor(Date.now() / 1000) },
      });
      return;
    }
    announce(t("teams.joined", { name: lookup.name }));
  };

  const withdraw = async (lookup: TeamLookup) => {
    setBusy(true);
    const done = await attempt(() => api.social.withdrawTeamRequest(lookup.teamId));
    setBusy(false);
    if (done === undefined) return;
    announce(t("crewWave2.join.withdrawn"));
    setFound({ kind: "found", lookup: { ...lookup, requestedAt: undefined } });
  };

  const offerOf = (lookup: TeamLookup) =>
    joinOffer(lookup, {
      requests: features.requests,
      invited: invites.some((invite) => invite.teamId === lookup.teamId),
      ownRequests,
    });

  const status = (lookup: TeamLookup) => {
    const offer = offerOf(lookup);
    if (offer === "join" || offer === "ask" || offer === "waiting") return null;
    if (offer === "askLimit") return t("crewWave2.join.askLimit");
    return offer === "limit" ? t("crews.limit", { count: limit }) : t(`teams.join.${offer}`);
  };

  return (
    <section
      className={cn(
        "rounded-[var(--radius-panel)] bg-obsidian-950/40 p-4 ring-1 ring-white/8",
        !first && "mt-4",
      )}
    >
      <SectionTitle>{t(`${words}.title`)}</SectionTitle>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void look(value);
        }}
      >
        <label htmlFor={inputId} className="sr-only">
          {features.link ? t("crewWave2.join.field") : t("teams.join.code")}
        </label>
        <div className="flex gap-2">
          <input
            id={inputId}
            value={value}
            onPaste={() => {
              pasted.current = true;
            }}
            onChange={(event) => {
              // Codes only on a service without links, as before.
              const formatted = features.link
                ? formatJoinInput(event.target.value, showsLink(value))
                : formatTeamCodeInput(event.target.value);
              setValue(formatted);
              setInvalid(false);
              const fromPaste = pasted.current;
              pasted.current = false;
              if (fromPaste && crewKeyOf(formatted)) {
                void look(formatted);
                return;
              }
              if (found.kind !== "looking") setFound({ kind: "idle" });
            }}
            placeholder="EKP-XXXX-XXXX"
            spellCheck={false}
            autoComplete="off"
            aria-invalid={invalid}
            className={cn(
              fieldClass,
              "font-display text-[16px] font-medium tracking-[0.06em] tabular-nums",
            )}
          />
          <Button
            type="submit"
            variant="secondary"
            icon={<MagnifyingGlass size={15} />}
            disabled={found.kind === "looking"}
          >
            {t("teams.join.find")}
          </Button>
        </div>
      </form>
      <div className="mt-3">
        <AnimatePresence mode="popLayout" initial={false}>
          {invalid ? (
            <motion.p
              key="invalid"
              role="alert"
              className="text-[12.5px] text-flare-300"
              {...rise(reduce, 4, 0.2)}
            >
              {t(`${words}.invalid`)}
            </motion.p>
          ) : found.kind === "looking" ? (
            <motion.div key="looking" aria-hidden {...rise(reduce, 4, 0.2)}>
              <div className="flex items-center gap-3.5 rounded-[var(--radius-panel)] p-3 ring-1 ring-white/6">
                <Skeleton className="size-[52px] rounded-[var(--radius-head)]" />
                <div className="flex-1 space-y-2">
                  <Skeleton className="h-4 w-36" />
                  <Skeleton className="h-3 w-16" />
                </div>
              </div>
            </motion.div>
          ) : found.kind === "found" ? (
            <motion.div
              key={`found:${found.lookup.teamId}`}
              className="rounded-[var(--radius-panel)] bg-white/[0.04] p-3 ring-1 ring-white/8"
              {...rise(reduce, 6, 0.26)}
            >
              <CrewPreview
                name={found.lookup.name}
                tag={found.lookup.tag}
                look={found.lookup}
                motto={found.lookup.motto}
                meta={t("teams.members", { count: found.lookup.members })}
                size={52}
              />
              <FoundOffer
                offer={offerOf(found.lookup)}
                status={status(found.lookup)}
                busy={busy}
                onJoin={() => void join(found.lookup)}
                onWithdraw={() => void withdraw(found.lookup)}
              />
            </motion.div>
          ) : found.kind === "error" ? (
            <motion.p
              key="error"
              role="alert"
              className="flex items-start gap-2 px-1 text-[13px] text-flare-400"
              {...rise(reduce, 4, 0.2)}
            >
              <WarningCircle size={16} weight="fill" className="mt-px shrink-0" aria-hidden />
              {found.message}
            </motion.p>
          ) : null}
        </AnimatePresence>
      </div>
    </section>
  );
}

/**
 * The found crew's button: "Ekibe katıl", "Katılma isteği gönder" (its leaders approve newcomers;
 * one line says so), "İsteğin bekliyor" with "Geri çek", or the line saying why neither goes. The
 * button turns into the waiting line in place once the request is sent.
 */
function FoundOffer({
  offer,
  status,
  busy,
  onJoin,
  onWithdraw,
}: {
  offer: ReturnType<typeof joinOffer>;
  status: string | null;
  busy: boolean;
  onJoin: () => void;
  onWithdraw: () => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  return (
    <AnimatePresence mode="popLayout" initial={false}>
      {status ? (
        <motion.p key="status" className="mt-3 text-[12.5px] text-ash-300" {...swap(reduce, 0.2)}>
          {status}
        </motion.p>
      ) : offer === "waiting" ? (
        <motion.div key="waiting" className="mt-3 flex items-center gap-2" {...swap(reduce, 0.2)}>
          <span className="flex min-w-0 flex-1 items-center gap-2 text-[13px] font-medium text-gold-200">
            <Clock size={15} weight="bold" className="shrink-0" aria-hidden />
            {t("crewWave2.join.waiting")}
          </span>
          <Button size="sm" variant="ghost" disabled={busy} onClick={onWithdraw}>
            {t("crewWave2.join.withdraw")}
          </Button>
        </motion.div>
      ) : (
        <motion.div key={offer} {...swap(reduce, 0.2)}>
          <Button variant="primary" className="mt-3 w-full" disabled={busy} onClick={onJoin}>
            {offer === "ask" ? t("crewWave2.join.ask") : t("teams.join.submit")}
          </Button>
          {offer === "ask" ? (
            <p className="mt-2 text-center text-[12.5px] text-ash-400">
              {t("crewWave2.join.askNote")}
            </p>
          ) : null}
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/** "Ekip kodu" with copy; the leader can renew it (asked once, in place). Under it, the invite
 * link and "Katılım" (TEAM5, crew-link.tsx). */
function CodeRow({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const [copied, setCopied] = useState(false);
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const rotate = async () => {
    setBusy(true);
    const code = await attempt(() => api.social.rotateTeamCode(team.id));
    setBusy(false);
    setAsking(false);
    if (code) announce(t("teams.code.rotated"));
  };
  return (
    <section className="mt-5 rounded-[var(--radius-panel)] bg-obsidian-950/40 px-4 py-3 ring-1 ring-white/8">
      <div className="flex items-center gap-3">
        <div className="min-w-0 flex-1">
          <p className="text-[12px] text-ash-400">{t("teams.code.label")}</p>
          <p className="relative h-7 overflow-hidden font-display text-[19px] leading-7 font-semibold tracking-[0.05em] text-bone-50 tabular-nums">
            <AnimatePresence mode="popLayout" initial={false}>
              <motion.span
                key={team.code}
                className="absolute inset-0"
                data-selectable
                {...swap(reduce, 0.26)}
              >
                {team.code}
              </motion.span>
            </AnimatePresence>
          </p>
        </div>
        <Button
          size="sm"
          variant="secondary"
          onClick={() =>
            void copyText(team.code).then((ok) => {
              if (!ok) return;
              setCopied(true);
              announce(t("teams.code.copied"));
              window.setTimeout(() => setCopied(false), 1600);
            })
          }
        >
          <span className="relative inline-flex items-center">
            <AnimatePresence mode="popLayout" initial={false}>
              <motion.span
                key={String(copied)}
                className="flex items-center gap-1.5"
                {...swap(reduce, 0.16)}
              >
                {copied ? (
                  <Check size={14} weight="bold" className="text-jade-400" />
                ) : (
                  <Copy size={14} />
                )}
                {copied ? t("teams.code.copied") : t("teams.code.copy")}
              </motion.span>
            </AnimatePresence>
          </span>
        </Button>
      </div>
      {team.role === "founder" ? (
        <div className="mt-2">
          <AnimatePresence mode="popLayout" initial={false}>
            {asking ? (
              <motion.div key="ask" className="flex items-center gap-2" {...rise(reduce, 4, 0.2)}>
                <p className="min-w-0 flex-1 text-[13px] text-bone-100">
                  {t("teams.code.rotateAsk")}
                </p>
                <Button size="sm" variant="ghost" onClick={() => setAsking(false)}>
                  {t("teams.cancel")}
                </Button>
                <Button size="sm" variant="secondary" disabled={busy} onClick={() => void rotate()}>
                  {t("teams.code.rotate")}
                </Button>
              </motion.div>
            ) : (
              <motion.div key="idle" {...rise(reduce, 4, 0.2)}>
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<ArrowsClockwise size={14} />}
                  onClick={() => setAsking(true)}
                >
                  {t("teams.code.rotate")}
                </Button>
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      ) : null}
      <CrewLinkRows team={team} />
    </section>
  );
}

/** The crew's "Düzenle": the leader edits everything, a moderator the look and the motto. */
function EditCrew({ team, onDone }: { team: SocialTeam; onDone: () => void }) {
  const { t } = useTranslation();
  const crew = useCrewFeatures();
  const texts = team.role === "founder";
  const [draft, setDraft] = useState<CrewDraft>(() => draftOf(team));
  const [busy, setBusy] = useState(false);
  const ready = draftReady(draft, crew.look, texts);

  const save = async () => {
    if (!ready || busy) return;
    const before = draftOf(team);
    const change: TeamChange = {};
    if (texts) {
      const name = cleanCrewText(draft.name);
      if (name !== team.name) change.name = name;
      if (draft.tag !== team.tag) change.tag = draft.tag;
    }
    if (draft.emblem !== before.emblem) change.emblem = draft.emblem;
    if (crew.look) {
      if (draft.color !== before.color) change.color = draft.color;
      if (draft.frame !== before.frame) change.frame = draft.frame;
      const motto = cleanCrewText(draft.motto);
      if (motto !== team.motto) change.motto = motto;
    }
    if (!Object.keys(change).length) {
      onDone();
      return;
    }
    setBusy(true);
    const done = await attempt(() => api.social.editTeam(team.id, change));
    setBusy(false);
    if (done === undefined) return;
    announce(t("teams.edit.saved"));
    onDone();
  };

  return (
    <CrewEditor
      draft={draft}
      onChange={setDraft}
      look={crew.look}
      texts={texts}
      meta={t("teams.membersOf", { count: team.members.length, limit: team.limit })}
      showErrors
      onSubmit={() => void save()}
      footer={
        <div className="flex justify-end gap-2 pt-1">
          <Button type="button" size="sm" variant="ghost" onClick={onDone}>
            {t("teams.cancel")}
          </Button>
          <Button type="submit" size="sm" variant="primary" disabled={busy || !ready}>
            {t("teams.edit.save")}
          </Button>
        </div>
      }
    />
  );
}

/**
 * "Ekibi çağır" (this crew), soft while the five minutes since the last call run (it says how
 * long); beside it, with two or more crews to call, "Birden çok ekibi çağır" (the picker, this
 * crew ticked).
 */
function CallButton({ team, onCall }: { team: SocialTeam; onCall: () => void }) {
  const { t } = useTranslation();
  const now = useCallClock((s) => s.now);
  const calls = useSocialUi((s) => s.view.calls);
  const crews = useSocialUi((s) => s.view.crews);
  const features = useCrewFeatures();
  const refusedUntil = useSocialUi((s) => s.callRefusedUntil);
  const lastCallAt = useSocialUi((s) => s.lastCallAt);
  const openPicker = useSocialUi((s) => s.openCallPicker);
  const until = callWaitUntil(calls, refusedUntil, lastCallAt);
  const waiting = until > now;
  const several = features.multi && callableCrews(crews, features).length > 1;
  return (
    <span className="inline-flex shrink-0 items-center gap-0.5">
      <Tooltip
        content={t("teams.call.wait", { count: waitMinutes(until, now) })}
        disabled={!waiting}
      >
        <span className="inline-flex">
          <Button
            size="sm"
            variant="secondary"
            icon={<MegaphoneSimple size={15} />}
            disabled={waiting}
            onClick={onCall}
          >
            {t("teams.call.start")}
          </Button>
        </span>
      </Tooltip>
      {several ? (
        <IconButton
          size="sm"
          label={t("crews.call.several")}
          icon={<CaretDown size={13} weight="bold" />}
          disabled={waiting}
          onClick={() => openPicker([team.id])}
        />
      ) : null}
    </span>
  );
}

/** The card's strip: who is online and "Ekibi çağır" (with the reason it cannot go yet), or the
 * calls on in this crew. */
function CrewStrip({ team, canCall }: { team: SocialTeam; canCall: boolean }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const now = useCallClock((s) => s.now);
  const crews = useSocialUi((s) => s.view.crews);
  const calls = liveCalls(
    useSocialUi((s) => s.view.calls),
    now,
  )
    .filter((call) => callNames(call, team, crews))
    .sort((a, b) => Number(b.mine) - Number(a.mine) || b.startedAt - a.startedAt);
  const online = team.members.filter((m) => !m.me && !m.blocked && m.state !== "offline");
  const { reason, call } = useCallPress();
  return (
    <AnimatePresence mode="popLayout" initial={false}>
      {calls.length ? (
        <motion.div
          key={`calls:${calls.map((c) => c.id).join()}`}
          className="space-y-4"
          {...rise(reduce, 0, 0.22)}
        >
          {calls.map((open) => (
            <CallStrip key={open.id} call={open} team={team} />
          ))}
        </motion.div>
      ) : (
        <motion.div key="online" {...rise(reduce, 0, 0.22)}>
          <div className="flex min-h-8 items-center gap-2.5">
            {online.length ? (
              <span className="flex items-center">
                {online.slice(0, 4).map((member, index) => (
                  <span
                    key={member.id}
                    className="relative flex rounded-[5px] ring-2 ring-obsidian-900"
                    style={{ marginLeft: index ? -7 : 0, zIndex: 4 - index }}
                  >
                    <SocialFace source={member} size={24} />
                  </span>
                ))}
              </span>
            ) : null}
            <span className="min-w-0 flex-1 truncate text-[13px] text-ash-300">
              {online.length ? (
                <>
                  <span className="relative inline-flex font-display font-semibold text-jade-400 tabular-nums">
                    <AnimatePresence mode="popLayout" initial={false}>
                      <motion.span key={online.length} {...swap(reduce, 0.18)}>
                        {online.length}
                      </motion.span>
                    </AnimatePresence>
                  </span>{" "}
                  {t("teams.card.online")}
                </>
              ) : (
                t("social.account.noneOnline")
              )}
            </span>
            {canCall ? <CallButton team={team} onCall={() => void call([team.id])} /> : null}
          </div>
          {canCall ? <CallReason reason={reason} crews={[team.id]} className="mt-2.5" /> : null}
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function CrewCard({
  team,
  canEdit,
  canCall,
  onEdit,
}: {
  team: SocialTeam;
  canEdit: boolean;
  canCall: boolean;
  onEdit: () => void;
}) {
  const { t } = useTranslation();
  const founded = localDay(team.createdOn);
  const anniversary = crewAnniversary(team.createdOn, new Date());
  const together = togetherTime(team.togetherWeek ?? 0);
  return (
    <div className="relative">
      <CrewWatermark look={team} />
      <div className="relative flex items-start gap-3.5">
        <CrewMark look={team} size={64} />
        <div className="min-w-0 flex-1">
          <p className="flex items-center gap-2 pr-1">
            <span className="truncate font-display text-[20px] leading-tight font-semibold text-bone-50">
              {team.name}
            </span>
            <TeamTag tag={team.tag} color={team.color} />
          </p>
          {team.motto ? (
            <p className="mt-0.5 text-[13px] leading-snug text-ash-300" data-selectable>
              {team.motto}
            </p>
          ) : null}
          <p className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[12.5px] text-ash-400">
            <span className="inline-flex items-center gap-1">
              <UsersThree size={14} aria-hidden />
              {t("teams.membersOf", { count: team.members.length, limit: team.limit })}
            </span>
            {founded ? (
              <span className="inline-flex items-center gap-1">
                <CalendarBlank size={14} aria-hidden />
                {t("teams.card.founded", {
                  date: dateTimeFormat({ day: "numeric", month: "long", year: "numeric" }).format(
                    founded,
                  ),
                })}
              </span>
            ) : null}
            {together ? (
              <span className="inline-flex items-center gap-1">
                <Clock size={14} aria-hidden />
                {t("crewWave2.together.week", { time: together })}
              </span>
            ) : null}
          </p>
          {anniversary ? (
            <p className="mt-1.5 flex items-center gap-1.5 text-[12.5px] font-medium text-gold-300">
              <Sparkle size={14} weight="fill" aria-hidden />
              {"years" in anniversary
                ? t("teams.card.yearsToday", { count: anniversary.years })
                : t("teams.card.monthsToday", { count: anniversary.months })}
            </p>
          ) : null}
        </div>
        {canEdit ? (
          <IconButton
            label={t("teams.edit.action")}
            icon={<PencilSimple size={16} />}
            size="sm"
            className="-mt-1 -mr-1"
            onClick={onEdit}
          />
        ) : null}
      </div>
      <div className="relative mt-4 border-t border-white/7 pt-3">
        <CrewStrip team={team} canCall={canCall} />
      </div>
    </div>
  );
}

/** The player's own row: their face, their badge, "Sen". */
function MeRow({ member }: { member: TeamMember }) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center gap-3 py-1.5 pr-2 pl-2">
      <SocialAvatar source={member} size={32} />
      <span className="flex min-w-0 flex-1 items-center gap-1.5">
        <span className="truncate text-[14px] font-semibold text-bone-50">{member.name}</span>
        <RoleBadge member={member} />
      </span>
      <span className="text-[12px] text-ash-400">{t("teams.you")}</span>
    </div>
  );
}

type RosterItem =
  | { key: string; kind: "group"; label: string; count: number }
  | { key: string; kind: "member"; member: TeamMember };

/**
 * Everyone, grouped by where they are; inside a group the leader, the moderators, then names. One
 * list, so someone who joins Projects glides into its group instead of vanishing and reappearing.
 */
function CrewRoster({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const activity = useSocialUi((s) => s.view.activity);
  const items = useMemo<RosterItem[]>(
    () =>
      rosterGroups(team, activity).flatMap(({ where, members }) => [
        {
          key: `g:${where}`,
          kind: "group" as const,
          label: t(`teams.roster.${where}`),
          count: members.length,
        },
        ...members.map((member) => ({ key: `m:${member.id}`, kind: "member" as const, member })),
      ]),
    [team, activity, t],
  );
  return (
    <ul aria-label={team.name} className="relative flex flex-col">
      <AnimatePresence initial={false} mode="popLayout">
        {items.map((item) => (
          <motion.li
            key={item.key}
            layout={layout ? "position" : false}
            transition={{ layout: glide }}
            initial={{ opacity: 0, y: reduce ? 0 : 6 }}
            animate={{ opacity: 1, y: 0, transition: enter(0.26) }}
            exit={{ opacity: 0, transition: leave(0.22) }}
          >
            {item.kind === "group" ? (
              <h3 className="flex items-center gap-2 px-2 pt-3 pb-1 text-[12.5px] font-medium text-ash-400">
                {item.label}
                <span className="relative inline-flex font-display tabular-nums">
                  <AnimatePresence mode="popLayout" initial={false}>
                    <motion.span key={item.count} {...swap(reduce, 0.18)}>
                      {item.count}
                    </motion.span>
                  </AnimatePresence>
                </span>
              </h3>
            ) : item.member.me ? (
              <MeRow member={item.member} />
            ) : (
              <TeamMemberRow member={item.member} team={team} line="where" />
            )}
          </motion.li>
        ))}
      </AnimatePresence>
    </ul>
  );
}

/** Friends to invite: those not in the crew, "Davet edildi" once asked. */
function InviteFriends({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const friends = useSocialUi((s) => s.view.friends);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState<string | null>(null);
  const members = new Set(team.members.map((m) => m.id));
  const candidates = friends
    .filter((f) => !members.has(f.id))
    .filter((f) => matchesQuery(f.name, query.trim()) || matchesQuery(f.username, query.trim()));
  const invite = async (friend: Friend) => {
    setBusy(friend.id);
    const done = await attempt(() => api.social.inviteToTeam(team.id, friend.id));
    setBusy(null);
    if (done !== undefined) announce(t("teams.invite.sent"));
  };
  const full = team.members.length >= team.limit;
  return (
    <section className="mt-5">
      <SectionTitle>{t("teams.invite.title")}</SectionTitle>
      {friends.filter((f) => !members.has(f.id)).length >= 8 ? (
        <div className="mb-2">
          <SearchInput
            value={query}
            onChange={setQuery}
            placeholder={t("teams.invite.search")}
            label={t("teams.invite.search")}
            clearLabel={t("social.list.clearSearch")}
          />
        </div>
      ) : null}
      {candidates.length === 0 ? (
        <p className="px-1 text-[13px] text-ash-400">{t("teams.invite.none")}</p>
      ) : (
        <ul className="max-h-[280px] overflow-y-auto">
          {candidates.map((friend) => {
            const invited = team.invited.includes(friend.id);
            return (
              // A friend here has the menu they have everywhere (FIX2: right click, Shift+F10).
              <PersonContextMenu key={friend.id} person={friend} place={{ from: "team" }}>
                <li className="group/invite flex items-center gap-3 rounded-[var(--radius-panel)] py-1.5 pr-1 pl-2 transition-colors duration-150 hover:bg-white/[0.035] has-[[data-state=open]]:bg-white/[0.05]">
                  <SocialAvatar source={friend} size={32} state={friend.state} dnd={friend.dnd} />
                  <span className="min-w-0 flex-1 truncate text-[13.5px] font-semibold text-bone-100">
                    {friend.name}
                  </span>
                  <Button
                    size="sm"
                    variant={invited ? "ghost" : "secondary"}
                    icon={invited ? <Check size={14} weight="bold" /> : <UserPlus size={14} />}
                    disabled={invited || full || busy === friend.id}
                    onClick={() => void invite(friend)}
                    onKeyDown={(event) => {
                      openMenuFromKey(event);
                    }}
                  >
                    {invited ? t("teams.invite.sent") : t("teams.invite.action")}
                  </Button>
                </li>
              </PersonContextMenu>
            );
          })}
        </ul>
      )}
    </section>
  );
}

/**
 * "Yetkiler" (the leader, on a service with roles): each right has a switch for moderators and,
 * where members may have it, one for members. A right is a level (the lowest role that has it), so
 * the switches move together where they must: giving it to members gives it to moderators too,
 * taking it from moderators takes it from members too. Each switch saves at once.
 */
function CrewPermissions({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const crew = useCrewFeatures();
  // "İstekler ve bağlantı" (`admit`, TEAM5): moderators or the leader only.
  const rows: Array<keyof TeamPermissions> = crew.requests ? [...RIGHTS, "admit"] : [...RIGHTS];
  const [pending, setPending] = useState<Partial<TeamPermissions>>({});
  const levels: TeamPermissions = { ...team.permissions, ...pending };
  // A pending level stays until the crew says so (the answer and the update cross).
  useEffect(() => {
    setPending((current) => {
      const left = Object.fromEntries(
        Object.entries(current).filter(
          ([right, level]) => team.permissions[right as keyof TeamPermissions] !== level,
        ),
      );
      return Object.keys(left).length === Object.keys(current).length ? current : left;
    });
  }, [team.permissions]);

  const change = async (
    right: keyof TeamPermissions,
    role: "moderators" | "members",
    on: boolean,
  ) => {
    const next = switchLevel(levels[right] ?? "moderators", role, on);
    if (next === levels[right]) return;
    setPending((current) => ({ ...current, [right]: next }));
    const done = await attempt(() =>
      api.social.editTeam(team.id, { permissions: { [right]: next } }),
    );
    if (done === undefined) {
      setPending((current) => {
        const { [right]: _, ...rest } = current;
        return rest;
      });
      return;
    }
    announce(t("teams.roles.saved"));
  };

  return (
    <section className="mt-5">
      <SectionTitle>{t("teams.roles.title")}</SectionTitle>
      <div className="rounded-[var(--radius-panel)] bg-obsidian-950/40 px-3 py-1.5 ring-1 ring-white/8">
        <table className="w-full border-collapse">
          <thead>
            <tr className="text-[12px] text-ash-400">
              <th scope="col" className="py-1.5 text-left font-medium">
                <span className="sr-only">{t("teams.roles.title")}</span>
              </th>
              <th scope="col" className="w-[92px] py-1.5 text-center font-medium">
                {t("teams.roles.moderators")}
              </th>
              <th scope="col" className="w-[64px] py-1.5 text-center font-medium">
                {t("teams.roles.members")}
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((right) => {
              const on = switchesOf(levels[right] ?? "moderators");
              const label = right === "admit" ? t("crewWave2.admit") : t(`teams.roles.${right}`);
              return (
                <tr key={right} className="border-t border-white/6">
                  <th
                    scope="row"
                    className="py-2.5 pr-2 text-left text-[13.5px] font-normal text-bone-100"
                  >
                    {label}
                  </th>
                  <td className="py-2.5">
                    <Switch
                      className="justify-center"
                      hideLabel
                      label={`${label}: ${t("teams.roles.moderators")}`}
                      checked={on.moderators}
                      onCheckedChange={(value) => void change(right, "moderators", value)}
                    />
                  </td>
                  <td className="py-2.5">
                    {right !== "admit" && membersMayHave(right as Right) ? (
                      <Switch
                        className="justify-center"
                        hideLabel
                        label={`${label}: ${t("teams.roles.members")}`}
                        checked={on.members}
                        onCheckedChange={(value) => void change(right, "members", value)}
                      />
                    ) : null}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </section>
  );
}

/** How long the page takes to slide in (motion.ts `pageVariants`), plus a frame. */
const PAGE_SETTLE_MS = 340;

/** The crew the page shows now, as `shownCrew` decides (several crews), else the only one; null
 * on "Ekip kur ya da katıl" and without a crew. */
export function useShownCrewId(): string | null {
  return useSocialUi((s) => {
    const shown = s.view.crew.multi
      ? shownCrew(s.view.crews, s.crewTab)
      : (s.view.crews[0] ?? NEW_CREW_TAB);
    return shown === NEW_CREW_TAB ? null : shown.id;
  });
}

function CrewView({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const crew = useCrewFeatures();
  const rights = crewRights(team, crew);
  const [editing, setEditing] = useState(false);
  const [asking, setAsking] = useState<Leaving | null>(null);
  const [busy, setBusy] = useState(false);
  const leaving = leavingOf(team);
  const inviteRef = useRef<HTMLDivElement>(null);
  const leaveRef = useRef<HTMLDivElement>(null);
  // Leaving the page for another crew, it never takes the next crew's menu intent.
  const current = useShownCrewId() === team.id;
  // Rights taken away while editing close the editor (the crew says so live).
  useEffect(() => {
    if (!rights.look) setEditing(false);
  }, [rights.look]);

  // A menu in the list sent the player here to edit, invite or leave: the page opens there.
  const intent = useSocialUi((s) => s.teamIntent);
  useEffect(() => {
    if (!intent || !current) return;
    useSocialUi.getState().takeTeamIntent();
    if (intent === "edit" && rights.look) setEditing(true);
    if (intent === "leave") setAsking(leaving);
    const place = intent === "invite" ? inviteRef : intent === "leave" ? leaveRef : null;
    if (!place) return;
    // After the page has slid in (its entrance scrolls the panel to the top). Taking the intent
    // re-runs this effect, so the timer is not cleared with it; gone, the ref is simply empty.
    window.setTimeout(
      () =>
        place.current?.scrollIntoView({ block: "center", behavior: reduce ? "auto" : "smooth" }),
      PAGE_SETTLE_MS,
    );
  }, [intent, current, rights.look, leaving, reduce]);

  const run = async (what: Leaving) => {
    setBusy(true);
    const done = await attempt(() =>
      what === "disband" ? api.social.disbandTeam(team.id) : api.social.leaveTeam(team.id),
    );
    setBusy(false);
    setAsking(null);
    if (done === undefined) return;
    announce(what === "disband" ? t("teams.leave.disbanded") : t("teams.leave.done"));
  };
  const asks: Record<Leaving, string> = {
    leave: t("teams.leave.ask"),
    close: t("teams.leave.closeAsk"),
    disband: t("teams.leave.disbandAsk"),
  };
  const labels: Record<Leaving, string> = {
    leave: t("teams.leave.action"),
    close: t("teams.leave.close"),
    disband: t("teams.leave.disband"),
  };
  // Sections below the card glide when the card grows (a call) or shrinks.
  const glideBox = {
    layout: layout ? ("position" as const) : false,
    transition: { layout: glide },
  };

  return (
    <LayoutGroup>
      <div className="pt-1">
        <CrewResetNote team={team} />
        <section className="relative overflow-hidden rounded-[var(--radius-panel)] bg-obsidian-950/40 p-4 ring-1 ring-white/8">
          <AnimatePresence mode="popLayout" initial={false}>
            {editing ? (
              <motion.div key="edit" {...rise(reduce, 4, 0.22)}>
                <EditCrew team={team} onDone={() => setEditing(false)} />
              </motion.div>
            ) : (
              <motion.div key="card" {...rise(reduce, 4, 0.22)}>
                <CrewCard
                  team={team}
                  canEdit={rights.look}
                  canCall={rights.call}
                  onEdit={() => setEditing(true)}
                />
              </motion.div>
            )}
          </AnimatePresence>
        </section>

        <motion.div {...glideBox}>
          <CrewRequests team={team} />
        </motion.div>

        <motion.div {...glideBox}>
          <CrewPlans team={team} />
        </motion.div>

        <motion.section className="mt-3" {...glideBox}>
          <CrewRoster team={team} />
        </motion.section>

        {rights.invite ? (
          <motion.div ref={inviteRef} {...glideBox}>
            <InviteFriends team={team} />
          </motion.div>
        ) : null}

        <motion.div {...glideBox}>
          <CodeRow team={team} />
        </motion.div>

        {rights.lead && crew.roles ? (
          <motion.div {...glideBox}>
            <CrewPermissions team={team} />
          </motion.div>
        ) : null}

        <motion.p className="mt-5 px-1 text-[12px] leading-snug text-ash-400" {...glideBox}>
          {t("teams.privacy")}
        </motion.p>

        <motion.div ref={leaveRef} className="mt-3 border-t border-white/8 pt-3" {...glideBox}>
          <AnimatePresence mode="popLayout" initial={false}>
            {asking ? (
              <motion.div key={asking} {...rise(reduce, 4, 0.2)}>
                <p className="text-[13px] leading-snug text-bone-100">{asks[asking]}</p>
                <div className="mt-2.5 flex justify-end gap-2">
                  <Button size="sm" variant="ghost" onClick={() => setAsking(null)}>
                    {t("teams.cancel")}
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    disabled={busy}
                    onClick={() => void run(asking)}
                  >
                    {labels[asking]}
                  </Button>
                </div>
              </motion.div>
            ) : (
              <motion.div key="idle" {...rise(reduce, 4, 0.2)}>
                <Button
                  size="sm"
                  variant="ghost"
                  className="text-flare-400"
                  icon={leaving === "leave" ? <SignOut size={14} /> : <Trash size={14} />}
                  onClick={() => setAsking(leaving)}
                >
                  {labels[leaving]}
                </Button>
              </motion.div>
            )}
          </AnimatePresence>
        </motion.div>
      </div>
    </LayoutGroup>
  );
}

/**
 * Without a crew, or on "Ekip kur ya da katıl": invites to crews, then founding and joining by code
 * while one more fits, or one line at the crew limit.
 */
function AddCrew({ canAdd, limit }: { canAdd: boolean; limit: number }) {
  const { t } = useTranslation();
  const invites = useSocialUi((s) => s.view.teamInvites);
  // An invite link opened at the limit still shows its crew (and says why it cannot be joined).
  const linked = useSocialUi((s) => s.prefillLink !== null);
  const [keepJoin, setKeepJoin] = useState(linked);
  if (linked && !keepJoin) setKeepJoin(true);
  return (
    <div className="pt-1">
      {invites.length ? (
        <section className="mb-4">
          <SectionTitle>{t("teams.invites")}</SectionTitle>
          {invites.map((invite) => (
            <TeamInviteRow key={invite.teamId} invite={invite} />
          ))}
        </section>
      ) : null}
      <MyRequests />
      {canAdd ? (
        <>
          {/* An invite link the player opened leads: the crew it shows comes first. */}
          {keepJoin ? (
            <>
              <JoinCrew first />
              <div className="mt-4">
                <CreateCrew />
              </div>
            </>
          ) : (
            <>
              <CreateCrew />
              <JoinCrew />
            </>
          )}
        </>
      ) : (
        <>
          <p className="flex items-center gap-2.5 rounded-[var(--radius-panel)] bg-obsidian-950/40 px-4 py-3.5 text-[13px] text-ash-300 ring-1 ring-white/8">
            <UsersThree size={17} className="shrink-0 text-ash-400" aria-hidden />
            {t("crews.limit", { count: limit })}
          </p>
          {linked || keepJoin ? <JoinCrew /> : null}
        </>
      )}
      <p className="mt-4 px-1 text-[12px] leading-snug text-ash-400">{t("teams.privacy")}</p>
    </div>
  );
}

/** A crew with chat (CHAT1): "Ekip" (the crew) and "Sohbet" (its chat, with how many are unread). */
function CrewPanes({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const pane = useSocialUi((s) => s.crewPane);
  const setPane = useSocialUi((s) => s.setCrewPane);
  const unread = useSocialUi(
    (s) => s.view.chat?.conversations.find((c) => c.id === `c:${team.id}`)?.unread ?? 0,
  );
  const open = useSocialUi((s) => s.open && s.page === "team");
  return (
    <Tabs<CrewPane>
      value={pane}
      onValueChange={setPane}
      label={team.name}
      items={[
        { value: "crew", label: t("chat.tabs.crew") },
        { value: "chat", label: t("chat.tabs.chat"), count: unread > 0 ? unread : undefined },
      ]}
    >
      <TabPanel value="crew" className="overflow-visible pt-3">
        <CrewView team={team} />
      </TabPanel>
      <TabPanel value="chat" className="flex flex-col overflow-hidden pt-0">
        <CrewChat team={team} onScreen={open && pane === "chat"} />
      </TabPanel>
    </Tabs>
  );
}

/**
 * A crew switch slides the page in from the side the picked mark sits on and the old one out the
 * other way (fades only under reduced motion). `custom` carries the side to the leaving page too.
 */
function switchVariants(reduce: boolean) {
  const x = (px: number) => (reduce ? "none" : `translateX(${px}px)`);
  return {
    from: (side: number) => ({ opacity: 0, transform: x(16 * side) }),
    on: { opacity: 1, transform: x(0), transition: enter(0.26) },
    gone: (side: number) => ({ opacity: 0, transform: x(-10 * side), transition: leave(0.18) }),
  };
}

export function TeamPage() {
  const crews = useSocialUi((s) => s.view.crews);
  const multi = useSocialUi((s) => s.view.crew.multi);
  const limit = useSocialUi((s) => s.view.crewLimit);
  const tab = useSocialUi((s) => s.crewTab);
  const setTab = useSocialUi((s) => s.setCrewTab);
  const { reduce } = useSocialMotion();
  const shown = multi ? shownCrew(crews, tab) : (crews[0] ?? NEW_CREW_TAB);
  const canAdd = canAddCrew(crews, limit, multi);
  const shownKey = shown === NEW_CREW_TAB ? NEW_CREW_TAB : shown.id;
  const rail = multi && crews.length > 0;
  const chat = useSocialUi((s) => s.view.chat !== undefined);
  const pane = useSocialUi((s) => s.crewPane);
  // The chat fills the panel (its messages scroll, the composer stays at the bottom).
  const fill = chat && pane === "chat" && shown !== NEW_CREW_TAB;

  // A crew founded or joined from "Ekip kur ya da katıl" comes on screen once it arrives.
  const count = useRef(crews.length);
  useEffect(() => {
    if (crews.length > count.current && tab === NEW_CREW_TAB) {
      const newest = crews.at(-1);
      if (newest) setTab(newest.id);
    }
    count.current = crews.length;
  }, [crews, tab, setTab]);

  // The side the page comes from: where the new mark sits against the old one in the rail (the
  // add view is last). Kept until the next switch, so a later update does not turn it around.
  const [side, setSide] = useState({ key: shownKey, side: 1 });
  if (side.key !== shownKey) {
    const order = [...crews.map((crew) => crew.id), NEW_CREW_TAB];
    setSide({
      key: shownKey,
      side: Math.sign(order.indexOf(shownKey) - order.indexOf(side.key)) || 1,
    });
  }
  // Another crew (or the add view) starts at the top, where its rail is: a crew founded or joined
  // from the long add view would otherwise open scrolled down.
  const root = useRef<HTMLDivElement>(null);
  const firstKey = useRef(shownKey);
  useEffect(() => {
    if (shownKey === firstKey.current) return;
    firstKey.current = shownKey;
    root.current?.closest("[data-social-scroller]")?.scrollTo({ top: 0 });
  }, [shownKey]);
  const slide = rail
    ? {
        variants: switchVariants(reduce),
        custom: side.side,
        initial: "from",
        animate: "on",
        exit: "gone",
      }
    : rise(reduce, 6, 0.28);

  return (
    <div ref={root} className={fill ? "flex h-full min-h-0 flex-col" : undefined}>
      {rail ? <CrewRail crews={crews} shown={shownKey} canAdd={canAdd} limit={limit} /> : null}
      <AnimatePresence mode="popLayout" initial={false} custom={side.side}>
        <motion.div
          key={shown === NEW_CREW_TAB ? "none" : `team:${shown.id}`}
          role={rail ? "tabpanel" : undefined}
          aria-labelledby={rail ? `crew-tab-${shownKey}` : undefined}
          className={fill ? "flex min-h-0 flex-1 flex-col" : undefined}
          {...slide}
        >
          {shown === NEW_CREW_TAB ? (
            <AddCrew canAdd={canAdd} limit={limit} />
          ) : chat ? (
            <CrewPanes team={shown} />
          ) : (
            <CrewView team={shown} />
          )}
        </motion.div>
      </AnimatePresence>
    </div>
  );
}
