// The team page of the friends panel. Without a team: invites to teams, "Takım kur" (name, tag,
// emblem, with a live preview of how it will look) and "Kodla katıl" (a code, then what it leads
// to). In a team: its card (emblem, name, tag, size, code), the members with their presence,
// friends to invite, and leaving (the founder edits, renews the code, hands the role on or
// disbands instead). Every irreversible step asks once, in place, with one sentence.
import {
  ArrowsClockwise,
  Check,
  Copy,
  Crown,
  MagnifyingGlass,
  PencilSimple,
  SignOut,
  Trash,
  UserPlus,
  WarningCircle,
} from "@phosphor-icons/react";
import { Button, cn, SearchInput, Skeleton } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type FormEvent, type KeyboardEvent, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce, friendlyError } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { Friend, SocialTeam, TeamLookup } from "@/lib/api/social-types";
import { matchesQuery } from "@/lib/search";
import { attempt, copyText } from "./actions";
import { rise, swap, useSocialMotion } from "./motion";
import { SocialAvatar } from "./social-face";
import { useSocialUi } from "./store";
import { TeamEmblem, TeamTag } from "./team-emblem";
import { TeamMemberRow } from "./team-member";
import { TeamInviteRow } from "./team-section";
import {
  EMBLEMS,
  type Emblem,
  formatTeamCodeInput,
  formatTeamTag,
  normalizeTeamCode,
  rosterOrder,
  TEAM_NAME_MAX,
  TEAM_TAG_MAX,
  teamNameProblem,
  teamTagValid,
} from "./teams";

const fieldClass =
  "h-10 w-full min-w-0 rounded-md border border-white/10 bg-obsidian-800/80 px-3 text-[14px] text-bone-50 transition-[border-color,box-shadow] duration-150 outline-none placeholder:text-ash-500 hover:border-white/16 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-300/25 aria-[invalid=true]:border-flare-400/70";

function SectionTitle({ children }: { children: string }) {
  return <h3 className="mb-2.5 text-[12.5px] font-medium text-ash-400">{children}</h3>;
}

/** The emblem picker: a radio group of 20 tiles, arrow keys move, the chosen one is gold-framed. */
function EmblemPicker({ value, onChange }: { value: Emblem; onChange: (emblem: Emblem) => void }) {
  const { t } = useTranslation();
  const group = useRef<HTMLDivElement>(null);
  const onKey = (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const columns = 10;
    const step =
      event.key === "ArrowRight"
        ? 1
        : event.key === "ArrowLeft"
          ? -1
          : event.key === "ArrowDown"
            ? columns
            : event.key === "ArrowUp"
              ? -columns
              : 0;
    if (!step) return;
    event.preventDefault();
    const next = EMBLEMS[(index + step + EMBLEMS.length) % EMBLEMS.length] as Emblem;
    onChange(next);
    group.current?.querySelector<HTMLElement>(`[data-emblem="${next}"]`)?.focus();
  };
  return (
    <div
      ref={group}
      role="radiogroup"
      aria-label={t("teams.create.emblem")}
      className="grid grid-cols-10 gap-1.5"
    >
      {EMBLEMS.map((emblem, index) => {
        const chosen = emblem === value;
        return (
          // biome-ignore lint/a11y/useSemanticElements: an icon tile; the group handles arrows (APG radio group).
          <button
            key={emblem}
            type="button"
            role="radio"
            aria-checked={chosen}
            aria-label={t(`teams.emblems.${emblem}`)}
            data-emblem={emblem}
            tabIndex={chosen ? 0 : -1}
            onClick={() => onChange(emblem)}
            onKeyDown={(event) => onKey(event, index)}
            className={cn(
              "flex aspect-square cursor-pointer items-center justify-center rounded-md outline-none transition-[background-color,box-shadow,transform,scale] duration-150 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.94]",
              chosen
                ? "bg-gold-500/15 ring-1 ring-gold-400/60"
                : "bg-white/[0.035] ring-1 ring-white/8 hover:bg-white/[0.07]",
            )}
          >
            <TeamEmblem emblem={emblem} size={28} className="bg-transparent ring-0" />
          </button>
        );
      })}
    </div>
  );
}

/** The card a team gets: emblem, name, tag. Live while it is being made. */
function TeamCardPreview({
  name,
  tag,
  emblem,
  meta,
  placeholder = false,
}: {
  name: string;
  tag: string;
  emblem: string;
  meta: string;
  /** No name typed yet: the name's place says what goes there, quietly. */
  placeholder?: boolean;
}) {
  const { reduce } = useSocialMotion();
  return (
    <div className="flex items-center gap-3.5">
      <AnimatePresence mode="popLayout" initial={false}>
        <motion.span key={emblem} className="flex" {...swap(reduce, 0.2)}>
          <TeamEmblem emblem={emblem} size={52} />
        </motion.span>
      </AnimatePresence>
      <div className="min-w-0">
        <p className="flex items-center gap-2">
          <span
            className={cn(
              "truncate font-display text-[19px] leading-tight font-semibold transition-colors duration-150",
              placeholder ? "text-ash-400" : "text-bone-50",
            )}
          >
            {name}
          </span>
          {tag ? <TeamTag tag={tag} /> : null}
        </p>
        <p className="mt-0.5 text-[12.5px] text-ash-400">{meta}</p>
      </div>
    </div>
  );
}

function CreateTeam() {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const nameId = useId();
  const tagId = useId();
  const [name, setName] = useState("");
  const [tag, setTag] = useState("");
  const [emblem, setEmblem] = useState<Emblem>("shield");
  const [touched, setTouched] = useState(false);
  const [busy, setBusy] = useState(false);
  const problem = teamNameProblem(name);
  const tagOk = teamTagValid(tag);
  const ready = !problem && tagOk;

  const submit = async (event: FormEvent) => {
    event.preventDefault();
    setTouched(true);
    if (!ready) return;
    setBusy(true);
    const done = await attempt(() => api.social.createTeam({ name: name.trim(), tag, emblem }));
    setBusy(false);
    if (done !== undefined) announce(t("teams.create.done", { name: name.trim() }));
  };

  const nameError = touched && problem ? t(`teams.create.${problem}`) : null;
  const tagError = touched && !tagOk ? t("teams.create.tagInvalid") : null;

  return (
    <section className="rounded-[var(--radius-panel)] bg-obsidian-950/40 p-4 ring-1 ring-white/8">
      <SectionTitle>{t("teams.create.title")}</SectionTitle>
      <TeamCardPreview
        name={name.trim() || t("teams.create.name")}
        placeholder={!name.trim()}
        tag={tag}
        emblem={emblem}
        meta={t("teams.members", { count: 1 })}
      />
      <form className="mt-4 space-y-3" onSubmit={(event) => void submit(event)} noValidate>
        <div className="flex gap-2">
          <div className="min-w-0 flex-1">
            <label htmlFor={nameId} className="mb-1 block text-[12.5px] text-ash-400">
              {t("teams.create.name")}
            </label>
            <input
              id={nameId}
              value={name}
              maxLength={TEAM_NAME_MAX + 8}
              onChange={(event) => setName(event.target.value)}
              aria-invalid={Boolean(nameError)}
              autoComplete="off"
              spellCheck={false}
              className={fieldClass}
            />
          </div>
          <div className="w-[84px] shrink-0">
            <label htmlFor={tagId} className="mb-1 block text-[12.5px] text-ash-400">
              {t("teams.create.tag")}
            </label>
            <input
              id={tagId}
              value={tag}
              maxLength={TEAM_TAG_MAX}
              onChange={(event) => setTag(formatTeamTag(event.target.value))}
              aria-invalid={Boolean(tagError)}
              autoComplete="off"
              spellCheck={false}
              className={cn(fieldClass, "font-display tracking-[0.08em] uppercase")}
            />
          </div>
        </div>
        <AnimatePresence initial={false}>
          {nameError || tagError ? (
            <motion.p
              key="error"
              role="alert"
              className="text-[12.5px] text-flare-300"
              {...rise(reduce, 4, 0.2)}
            >
              {nameError ?? tagError}
            </motion.p>
          ) : null}
        </AnimatePresence>
        <div>
          <p className="mb-1.5 text-[12.5px] text-ash-400">{t("teams.create.emblem")}</p>
          <EmblemPicker value={emblem} onChange={setEmblem} />
        </div>
        <Button type="submit" variant="primary" className="w-full" disabled={busy}>
          {t("teams.create.submit")}
        </Button>
      </form>
    </section>
  );
}

type Found =
  | { kind: "idle" }
  | { kind: "looking" }
  | { kind: "found"; lookup: TeamLookup }
  | { kind: "error"; message: string };

function JoinTeam() {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const inputId = useId();
  const [value, setValue] = useState("");
  const [invalid, setInvalid] = useState(false);
  const [found, setFound] = useState<Found>({ kind: "idle" });
  const [busy, setBusy] = useState(false);
  const pasted = useRef(false);

  const look = async (input: string) => {
    const code = normalizeTeamCode(input);
    if (!code) {
      setInvalid(true);
      return;
    }
    setInvalid(false);
    setFound({ kind: "looking" });
    try {
      setFound({ kind: "found", lookup: await api.social.lookupTeam(code) });
    } catch (error) {
      setFound({
        kind: "error",
        message: friendlyError(error, t("social.actions.failed")).message,
      });
    }
  };

  const join = async (lookup: TeamLookup) => {
    setBusy(true);
    const done = await attempt(() => api.social.joinTeam(lookup.code));
    setBusy(false);
    if (done !== undefined) announce(t("teams.joined", { name: lookup.name }));
  };

  const status = (lookup: TeamLookup) =>
    lookup.relation === "none" ? null : t(`teams.join.${lookup.relation}`);

  return (
    <section className="mt-4 rounded-[var(--radius-panel)] bg-obsidian-950/40 p-4 ring-1 ring-white/8">
      <SectionTitle>{t("teams.join.title")}</SectionTitle>
      <form
        onSubmit={(event) => {
          event.preventDefault();
          void look(value);
        }}
      >
        <label htmlFor={inputId} className="sr-only">
          {t("teams.join.code")}
        </label>
        <div className="flex gap-2">
          <input
            id={inputId}
            value={value}
            onPaste={() => {
              pasted.current = true;
            }}
            onChange={(event) => {
              const formatted = formatTeamCodeInput(event.target.value);
              setValue(formatted);
              setInvalid(false);
              const fromPaste = pasted.current;
              pasted.current = false;
              if (fromPaste && normalizeTeamCode(formatted)) {
                void look(formatted);
                return;
              }
              if (found.kind !== "looking") setFound({ kind: "idle" });
            }}
            placeholder="TKM-XXXX-XXXX"
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
              {t("teams.join.invalid")}
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
              key={`found:${found.lookup.code}`}
              className="rounded-[var(--radius-panel)] bg-white/[0.04] p-3 ring-1 ring-white/8"
              {...rise(reduce, 6, 0.26)}
            >
              <TeamCardPreview
                name={found.lookup.name}
                tag={found.lookup.tag}
                emblem={found.lookup.emblem}
                meta={t("teams.members", { count: found.lookup.members })}
              />
              {status(found.lookup) ? (
                <p className="mt-3 text-[12.5px] text-ash-300">{status(found.lookup)}</p>
              ) : (
                <Button
                  variant="primary"
                  className="mt-3 w-full"
                  disabled={busy}
                  onClick={() => void join(found.lookup)}
                >
                  {t("teams.join.submit")}
                </Button>
              )}
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

/** "Takım kodu" with copy; the founder can renew it (asked once, in place). */
function CodeRow({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const [copied, setCopied] = useState(false);
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  const rotate = async () => {
    setBusy(true);
    const code = await attempt(() => api.social.rotateTeamCode());
    setBusy(false);
    setAsking(false);
    if (code) announce(t("teams.code.rotated"));
  };
  return (
    <div className="mt-4 border-t border-white/7 pt-3">
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
    </div>
  );
}

/** The founder's in-place edit of name, tag and emblem. */
function EditTeam({ team, onDone }: { team: SocialTeam; onDone: () => void }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const [name, setName] = useState(team.name);
  const [tag, setTag] = useState(team.tag);
  const [emblem, setEmblem] = useState<Emblem>(
    (EMBLEMS as readonly string[]).includes(team.emblem) ? (team.emblem as Emblem) : "shield",
  );
  const [busy, setBusy] = useState(false);
  const problem = teamNameProblem(name);
  const tagOk = teamTagValid(tag);
  const ready = !problem && tagOk;
  // As in "Takım kur": one line says what to fix (the tag too, not only the name), and it fades
  // in and out instead of snapping while the player types (POLISH6).
  const error = problem
    ? t(`teams.create.${problem}`)
    : tagOk
      ? null
      : t("teams.create.tagInvalid");
  const save = async () => {
    if (!ready) return;
    setBusy(true);
    const change = {
      ...(name.trim() !== team.name ? { name: name.trim() } : {}),
      ...(tag !== team.tag ? { tag } : {}),
      ...(emblem !== team.emblem ? { emblem } : {}),
    };
    const done = Object.keys(change).length
      ? await attempt(() => api.social.editTeam(change))
      : null;
    setBusy(false);
    if (done === undefined) return;
    if (Object.keys(change).length) announce(t("teams.edit.saved"));
    onDone();
  };
  return (
    <div className="space-y-3">
      <TeamCardPreview
        name={name.trim() || team.name}
        tag={tag}
        emblem={emblem}
        meta={t("teams.membersOf", { count: team.members.length, limit: team.limit })}
      />
      <div className="flex gap-2">
        <input
          aria-label={t("teams.create.name")}
          value={name}
          maxLength={TEAM_NAME_MAX + 8}
          onChange={(event) => setName(event.target.value)}
          aria-invalid={Boolean(problem)}
          className={fieldClass}
        />
        <input
          aria-label={t("teams.create.tag")}
          value={tag}
          maxLength={TEAM_TAG_MAX}
          onChange={(event) => setTag(formatTeamTag(event.target.value))}
          aria-invalid={!tagOk}
          className={cn(fieldClass, "w-[84px] shrink-0 font-display tracking-[0.08em]")}
        />
      </div>
      <AnimatePresence initial={false}>
        {error ? (
          <motion.p
            key="error"
            role="alert"
            className="text-[12.5px] text-flare-300"
            {...rise(reduce, 4, 0.2)}
          >
            {error}
          </motion.p>
        ) : null}
      </AnimatePresence>
      <EmblemPicker value={emblem} onChange={setEmblem} />
      <div className="flex justify-end gap-2">
        <Button size="sm" variant="ghost" onClick={onDone}>
          {t("teams.cancel")}
        </Button>
        <Button size="sm" variant="primary" disabled={busy || !ready} onClick={() => void save()}>
          {t("teams.edit.save")}
        </Button>
      </div>
    </div>
  );
}

/** Friends to invite: those not in the team, "Davet edildi" once asked. */
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
    const done = await attempt(() => api.social.inviteToTeam(friend.id));
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
              <li
                key={friend.id}
                className="flex items-center gap-3 rounded-[var(--radius-panel)] py-1.5 pr-1 pl-2"
              >
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
                >
                  {invited ? t("teams.invite.sent") : t("teams.invite.action")}
                </Button>
              </li>
            );
          })}
        </ul>
      )}
    </section>
  );
}

type Leaving = "leave" | "close" | "disband";

function TeamView({ team }: { team: SocialTeam }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const [editing, setEditing] = useState(false);
  const [asking, setAsking] = useState<Leaving | null>(null);
  const [busy, setBusy] = useState(false);
  const founder = team.role === "founder";
  const alone = team.members.length <= 1;
  const leaving: Leaving = founder ? (alone ? "close" : "disband") : "leave";
  const run = async (what: Leaving) => {
    setBusy(true);
    const done = await attempt(() =>
      what === "disband" ? api.social.disbandTeam() : api.social.leaveTeam(),
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
  return (
    <div className="pt-1">
      <section className="rounded-[var(--radius-panel)] bg-obsidian-950/40 p-4 ring-1 ring-white/8">
        <AnimatePresence mode="popLayout" initial={false}>
          {editing ? (
            <motion.div key="edit" {...rise(reduce, 4, 0.22)}>
              <EditTeam team={team} onDone={() => setEditing(false)} />
            </motion.div>
          ) : (
            <motion.div key="card" className="flex items-start gap-2" {...rise(reduce, 4, 0.22)}>
              <div className="min-w-0 flex-1">
                <TeamCardPreview
                  name={team.name}
                  tag={team.tag}
                  emblem={team.emblem}
                  meta={t("teams.membersOf", { count: team.members.length, limit: team.limit })}
                />
              </div>
              {founder ? (
                <Button
                  size="sm"
                  variant="ghost"
                  icon={<PencilSimple size={14} />}
                  onClick={() => setEditing(true)}
                >
                  {t("teams.edit.action")}
                </Button>
              ) : null}
            </motion.div>
          )}
        </AnimatePresence>
        <CodeRow team={team} />
      </section>

      <section className="mt-5">
        <ul aria-label={team.name} className="flex flex-col">
          {rosterOrder(team).map((member) =>
            member.me ? (
              <li key={member.id} className="flex items-center gap-3 py-2 pr-2 pl-2">
                <SocialAvatar source={member} size={36} />
                <span className="flex min-w-0 flex-1 items-center gap-1.5">
                  <span className="truncate text-[14px] font-semibold text-bone-50">
                    {member.name}
                  </span>
                  {member.role === "founder" ? (
                    <Crown
                      size={12}
                      weight="fill"
                      className="shrink-0 text-gold-300"
                      aria-label={t("teams.founder")}
                    />
                  ) : null}
                </span>
                <span className="text-[12px] text-ash-400">{t("teams.you")}</span>
              </li>
            ) : (
              <li key={member.id}>
                <TeamMemberRow member={member} founder={founder} />
              </li>
            ),
          )}
        </ul>
      </section>

      <InviteFriends team={team} />

      <p className="mt-5 px-1 text-[12px] leading-snug text-ash-400">{t("teams.privacy")}</p>

      <div className="mt-3 border-t border-white/8 pt-3">
        <AnimatePresence mode="popLayout" initial={false}>
          {asking ? (
            <motion.div key={asking} {...rise(reduce, 4, 0.2)}>
              <p className="text-[13px] leading-snug text-bone-100">{asks[asking]}</p>
              <div className="mt-2.5 flex justify-end gap-2">
                <Button size="sm" variant="ghost" onClick={() => setAsking(null)}>
                  {t("teams.cancel")}
                </Button>
                <Button size="sm" variant="danger" disabled={busy} onClick={() => void run(asking)}>
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
      </div>
    </div>
  );
}

export function TeamPage() {
  const { t } = useTranslation();
  const team = useSocialUi((s) => s.view.team);
  const invites = useSocialUi((s) => s.view.teamInvites);
  const { reduce } = useSocialMotion();
  return (
    <AnimatePresence mode="popLayout" initial={false}>
      {team ? (
        <motion.div key={`team:${team.id}`} {...rise(reduce, 6, 0.28)}>
          <TeamView team={team} />
        </motion.div>
      ) : (
        <motion.div key="none" className="pt-1" {...rise(reduce, 6, 0.28)}>
          {invites.length ? (
            <section className="mb-4">
              <SectionTitle>{t("teams.invites")}</SectionTitle>
              {invites.map((invite) => (
                <TeamInviteRow key={invite.teamId} invite={invite} />
              ))}
            </section>
          ) : null}
          <CreateTeam />
          <JoinTeam />
          <p className="mt-4 px-1 text-[12px] leading-snug text-ash-400">{t("teams.privacy")}</p>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
