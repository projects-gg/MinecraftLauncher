// "Arkadaşlarını çağır" (HOST1, host1.md §1.2, §1.3): the launcher's invite window, the same
// fields as the game's. Online friends, in game first; under each crew its members who are
// friends with "Hepsini seç"; the count against the guest limit ("3/7"); the game mode (the
// world's own first) and "Arkadaşların komut kullanabilsin" (off); once per computer the privacy
// line. "Dünyayı aç" closes the window and the world's row shows the opening in place; the quiet
// "Projects'te buluşun" invites the same friends to projects.gg instead. While the world is open
// the window is "Davet et": the friends already in the session show ticked and fixed. With the
// overlay switch off the window says so first, with "Aç", and glides into the list once it is on.
import { ShieldCheck, UsersThree } from "@phosphor-icons/react";
import { Button, ContentIcon, cn, Dialog, SearchInput, Segmented, Switch } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Check } from "@/features/settings/cleanup/check";
import { isJoinable } from "@/features/social/model";
import { PresenceText } from "@/features/social/presence-text";
import { crossfade, swap, useSocialMotion } from "@/features/social/motion";
import { SocialAvatar } from "@/features/social/social-face";
import { useSocialUi } from "@/features/social/store";
import { CrewMark } from "@/features/social/team-emblem";
import type { GuestState, WorldGameMode } from "@/lib/api/host-types";
import type { Friend, SocialTeam } from "@/lib/api/social-types";
import { usePrefs } from "@/stores/prefs";
import { inviteFriends, meetOnProjects, openWorld } from "./actions";
import { GuestStateText } from "./parts";
import {
  defaultMode,
  groupChoice,
  type PickerGroup,
  pickerCount,
  pickerGroups,
  toggleGroup,
  toggleOne,
} from "./rules";
import { type PickerRequest, useHostPrefs, useHostUi } from "./store";

/** From this many people the window gets a search field (a short list reads at a glance). */
const SEARCH_FROM = 9;

const MODES: readonly WorldGameMode[] = ["survival", "creative", "adventure"];

/** Crews are off: none to list (one array, so the lists are not built again on each render). */
const NO_CREWS: SocialTeam[] = [];

function PersonRow({
  friend,
  checked,
  fixed,
  disabled,
  onToggle,
}: {
  friend: Friend;
  checked: boolean;
  /** Already in the session: its state instead of a choice. */
  fixed?: GuestState;
  disabled: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  const off = disabled && !checked;
  return (
    <li>
      <label
        htmlFor={id}
        className={cn(
          "flex items-center gap-3 rounded-[var(--radius-panel)] px-2 py-1.5 transition-[background-color,opacity] duration-150",
          fixed || off ? "cursor-default" : "cursor-pointer hover:bg-white/[0.045]",
          checked && !fixed && "bg-gold-500/[0.06]",
          off && "opacity-55",
        )}
      >
        <Check
          id={id}
          checked={checked || Boolean(fixed)}
          disabled={Boolean(fixed) || off}
          onCheckedChange={onToggle}
        />
        <SocialAvatar
          source={friend}
          size={32}
          state={friend.state}
          dnd={friend.dnd}
          surface="var(--color-obsidian-900)"
        />
        <span className="min-w-0 flex-1">
          <span className="block truncate text-[13.5px] font-semibold text-bone-50">
            {friend.name}
          </span>
          <span
            className={cn(
              "block truncate text-[12px]",
              // As in the friend list: jade where "Katıl" would lead (projects.gg).
              friend.state === "playing" && isJoinable(friend.game)
                ? "text-jade-400"
                : "text-ash-400",
            )}
          >
            <PresenceText
              friend={friend}
              t={t}
              joinable={friend.state === "playing" && isJoinable(friend.game)}
            />
          </span>
        </span>
        {fixed ? <GuestStateText state={fixed} /> : null}
      </label>
    </li>
  );
}

function GroupHeader({
  group,
  choice,
  onToggle,
}: {
  group: PickerGroup;
  choice: "all" | "clear" | "none";
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  return (
    <div className="flex h-9 items-center gap-2 px-2 pt-2">
      {group.crew ? <CrewMark look={group.crew} size={18} /> : null}
      <span className="min-w-0 flex-1 truncate text-[12.5px] font-medium text-ash-400">
        {group.crew ? group.crew.name : t("host.picker.friends")}
      </span>
      {group.crew && choice !== "none" ? (
        <button
          type="button"
          onClick={onToggle}
          className="relative inline-grid h-7 cursor-pointer items-center rounded-md px-2 text-[12.5px] font-medium text-gold-300 outline-none transition-colors duration-150 hover:bg-white/6 hover:text-gold-200 focus-visible:ring-2 focus-visible:ring-gold-300"
        >
          {/* Both words share one cell: the button keeps its width when they swap. */}
          <span aria-hidden className="invisible [grid-area:1/1]">
            {t("host.picker.selectAll")}
          </span>
          <span aria-hidden className="invisible [grid-area:1/1]">
            {t("host.picker.clearAll")}
          </span>
          <AnimatePresence initial={false} mode="popLayout">
            <motion.span key={choice} className="[grid-area:1/1] text-center" {...swap(reduce)}>
              {choice === "clear" ? t("host.picker.clearAll") : t("host.picker.selectAll")}
            </motion.span>
          </AnimatePresence>
        </button>
      ) : null}
    </div>
  );
}

/** "3/7": the friends ticked (and already in the session) against the limit. */
function Counter({ count, max }: { count: number; max: number }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  return (
    <span
      role="status"
      className={cn(
        "mr-auto inline-flex items-center gap-1.5 font-display text-[15px] font-semibold tabular-nums transition-colors duration-150",
        count > 0 ? "text-gold-300" : "text-ash-400",
      )}
    >
      <UsersThree size={16} weight="bold" aria-hidden />
      <span aria-hidden className="relative inline-flex">
        <AnimatePresence initial={false} mode="popLayout">
          <motion.span key={count} {...swap(reduce, 0.16)}>
            {count}
          </motion.span>
        </AnimatePresence>
        /{max}
      </span>
      <span className="sr-only">{t("host.picker.countLabel", { count, max })}</span>
    </span>
  );
}

export function PickerDialog() {
  const { t } = useTranslation();
  const request = useHostUi((s) => s.picker);
  const close = useHostUi((s) => s.closePicker);
  const view = useHostUi((s) => s.view);
  const social = useSocialUi((s) => s.view);
  const overlayOn = usePrefs((s) => s.inGameOverlay);
  const setPref = usePrefs((s) => s.set);
  const privacySeen = useHostPrefs((s) => s.privacySeen);
  const setPrivacySeen = useHostPrefs((s) => s.setPrivacySeen);
  const commandsId = useId();
  // The window keeps its words while it fades out.
  const last = useRef<PickerRequest | null>(null);
  if (request) last.current = request;
  const shown = request ?? last.current;

  const [selected, setSelected] = useState<Set<string>>(() => new Set());
  const [mode, setMode] = useState<WorldGameMode>("survival");
  const [commands, setCommands] = useState(false);
  const [query, setQuery] = useState("");
  const [busy, setBusy] = useState(false);
  // The privacy line stays for this whole opening once it showed (it counts as seen on close).
  const [privacy, setPrivacy] = useState(false);
  // biome-ignore lint/correctness/useExhaustiveDependencies: only a new opening resets it
  useEffect(() => {
    if (!request) return;
    setSelected(new Set(request.kind === "open" ? (request.preselect ?? []) : []));
    setMode(request.kind === "open" ? defaultMode(request) : "survival");
    setCommands(false);
    setQuery("");
    setBusy(false);
    setPrivacy(request.kind === "open" && !privacySeen);
  }, [request]);

  const session = view.session?.phase === "open" ? view.session : null;
  const more = shown?.kind === "more";
  const taken = useMemo(
    () =>
      more && session
        ? new Map(session.guests.map((g) => [g.friend, g.state] as const))
        : new Map<string, GuestState>(),
    [more, session],
  );
  const takenIds = useMemo(() => new Set(taken.keys()), [taken]);
  const max = more && session ? session.max : view.maxGuests;
  const room = Math.max(0, max - takenIds.size);

  const blocked = useMemo(() => new Set(social.blocked.map((b) => b.id)), [social.blocked]);
  const crews = social.teams ? social.crews : NO_CREWS;
  const all = useMemo(
    () => pickerGroups({ friends: social.friends, crews, blocked, taken: takenIds, query: "" }),
    [social.friends, crews, blocked, takenIds],
  );
  const groups = useMemo(
    () =>
      query.trim()
        ? pickerGroups({ friends: social.friends, crews, blocked, taken: takenIds, query })
        : all,
    [all, query, social.friends, crews, blocked, takenIds],
  );
  const total = pickerCount(all);
  const searchable = total >= SEARCH_FROM;
  const full = selected.size >= room;
  const needsOverlay = !more && !overlayOn;

  const finish = () => {
    if (privacy) setPrivacySeen();
    close();
  };

  const submit = async () => {
    if (!shown || busy || selected.size === 0) return;
    const invite = [...selected];
    if (shown.kind === "open") {
      finish();
      await openWorld({
        instanceId: shown.instanceId,
        folder: shown.folder,
        mode,
        commands,
        invite,
      });
      return;
    }
    setBusy(true);
    const sent = await inviteFriends(invite);
    setBusy(false);
    if (sent) finish();
  };

  const meet = () => {
    if (selected.size === 0) return;
    const invite = [...selected];
    finish();
    void meetOnProjects(invite);
  };

  if (!shown) return null;

  const footer = needsOverlay ? (
    <>
      <Button variant="ghost" onClick={finish}>
        {t("fix2.connect.cancel")}
      </Button>
      <Button variant="primary" onClick={() => setPref("inGameOverlay", true)}>
        {t("host.overlay.on")}
      </Button>
    </>
  ) : (
    <>
      <Counter count={takenIds.size + selected.size} max={max} />
      {more ? null : (
        <Button variant="ghost" disabled={selected.size === 0} onClick={meet}>
          {t("host.picker.meet")}
        </Button>
      )}
      <Button
        variant="primary"
        disabled={selected.size === 0 || busy}
        onClick={() => void submit()}
      >
        {more ? t("host.picker.invite") : t("host.picker.open")}
      </Button>
    </>
  );

  return (
    <Dialog
      open={request !== null}
      onOpenChange={(open) => {
        if (!open) finish();
      }}
      title={t("host.picker.title")}
      description={
        shown.kind === "open"
          ? t("host.picker.world", { world: shown.worldName })
          : session
            ? t("host.picker.world", { world: session.worldName })
            : undefined
      }
      closeLabel={t("common.close")}
      width={480}
      morph
      footer={footer}
    >
      <AnimatePresence initial={false} mode="popLayout">
        {needsOverlay ? (
          <motion.div key="overlay" {...crossfade()}>
            <p className="flex items-start gap-3 rounded-[var(--radius-panel)] bg-obsidian-950/45 px-3.5 py-3 text-[13.5px] text-bone-100 ring-1 ring-white/8">
              <ContentIcon kind="world" size={16} className="mt-0.5" />
              {t("host.overlay.body")}
            </p>
          </motion.div>
        ) : (
          <motion.div key="picker" {...crossfade()}>
            {total === 0 ? (
              <div className="flex flex-col items-center px-4 pt-3 pb-5 text-center">
                <span className="chamfer mb-3 flex size-14 items-center justify-center bg-obsidian-800 [--cut:8px]">
                  <ContentIcon kind="friends" size={32} />
                </span>
                <p className="text-[13.5px] text-ash-300">{t("host.picker.empty")}</p>
              </div>
            ) : (
              <>
                {searchable ? (
                  <SearchInput
                    value={query}
                    onChange={setQuery}
                    placeholder={t("host.picker.search")}
                    label={t("host.picker.search")}
                    clearLabel={t("host.picker.clearSearch")}
                    className="mb-1"
                  />
                ) : null}
                <fieldset className="-mx-2 my-0 max-h-[min(46vh,336px)] min-w-0 overflow-y-auto overscroll-contain border-0 px-0.5 py-0">
                  <legend className="sr-only">{t("host.picker.friends")}</legend>
                  {groups.length === 0 ? (
                    <p className="px-2 py-6 text-center text-[13px] text-ash-400">
                      {t("host.picker.noMatch")}
                    </p>
                  ) : (
                    groups.map((group) => (
                      <section key={group.key}>
                        <GroupHeader
                          group={group}
                          choice={groupChoice(group.people, selected, takenIds)}
                          onToggle={() =>
                            setSelected((current) =>
                              toggleGroup(group.people, current, takenIds, room),
                            )
                          }
                        />
                        <ul className="flex flex-col gap-px">
                          {group.people.map((friend) => (
                            <PersonRow
                              key={friend.id}
                              friend={friend}
                              checked={selected.has(friend.id)}
                              fixed={taken.get(friend.id)}
                              disabled={full}
                              onToggle={() =>
                                setSelected((current) => toggleOne(current, friend.id, room))
                              }
                            />
                          ))}
                        </ul>
                      </section>
                    ))
                  )}
                </fieldset>
              </>
            )}
            {/* Nobody to invite: the choices of a session that cannot start mean nothing. */}
            {more || total === 0 ? null : (
              <div className="mt-3 space-y-3 border-t border-white/7 pt-3.5">
                <div className="flex items-center justify-between gap-4">
                  <span className="text-[13px] text-bone-100">{t("host.picker.mode")}</span>
                  <Segmented<WorldGameMode>
                    value={mode}
                    onValueChange={setMode}
                    label={t("host.picker.mode")}
                    options={MODES.map((value) => ({ value, label: t(`host.modes.${value}`) }))}
                  />
                </div>
                <div className="switch-row flex items-center justify-between gap-4">
                  <label htmlFor={commandsId} className="cursor-pointer text-[13px] text-bone-100">
                    {t("host.picker.commands")}
                  </label>
                  <Switch
                    id={commandsId}
                    hideLabel
                    checked={commands}
                    onCheckedChange={setCommands}
                    label={t("host.picker.commands")}
                  />
                </div>
                {privacy ? (
                  <p className="flex items-center gap-2 text-[12.5px] text-ash-300">
                    <ShieldCheck size={15} weight="fill" className="shrink-0 text-gold-300/90" />
                    {t("host.picker.privacy")}
                  </p>
                ) : null}
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </Dialog>
  );
}
