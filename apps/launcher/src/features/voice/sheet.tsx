// The room panel (VOICE-L3; voice.md §5.1): a sheet from the right, as the friends panel is. The
// header names the room (its leader renames it in place) with "Davet et", the settings and close;
// a crew room shows the crew's rooms under it as chips with their counts ("Genel 4", "Oda 2 1"),
// another room's chip joins it and "+" opens a new one. Then what needs the player (the microphone,
// a moderator's mute, the connection), the people (the player first, then the others as they came,
// then invites that ring with "İptal"), each with their speaking ring, role word and state marks;
// a press or a right click opens their menu (volume first). At the bottom "Mikrofon",
// "Sağırlaştır" and "Ayrıl", and for the leader "Odayı kapat" with its one sentence. "Davet et"
// slides in over the room as the friends panel's pages do.
import {
  ArrowsClockwise,
  CaretLeft,
  GearSix,
  Headphones,
  Keyboard,
  Microphone,
  MicrophoneSlash,
  PencilSimple,
  PhoneDisconnect,
  Plus,
  SpeakerHigh,
  SpeakerSimpleSlash,
  User,
  UserPlus,
  Warning,
  X,
} from "@phosphor-icons/react";
import {
  Button,
  ContextMenu,
  cn,
  EntranceGate,
  focusPanel,
  GatedMotion,
  IconButton,
  Input,
  Menu,
  type MenuItem,
  ReducedMotionConfig,
  SearchInput,
  SwapLabel,
  scrimMotion,
  sheetMotion,
  sheetOutside,
  Tooltip,
  useInteractionMotion,
  useMotionPrefs,
} from "@pl/ui";
import { useNavigate } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import { Dialog } from "radix-ui";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useRoomTargets } from "@/features/share/live";
import { LiveAppear, LiveTag, LiveTagButton, useLivePreview } from "@/features/share/live-card";
import { useUnshareItem, useWatchItem } from "@/features/share/menu";
import { ShareFooterRow, WatchButton } from "@/features/share/panel";
import { openMenuFromKey } from "@/features/social/menu-key";
import { enter, glide, leave, pageVariants, rise } from "@/features/social/motion";
import { usePersonItems } from "@/features/social/person-menu";
import { SocialFace } from "@/features/social/social-face";
import { useSocialUi } from "@/features/social/store";
import type { VoicePerson, VoiceRoomView } from "@/lib/api/voice-types";
import { formatPercent } from "@/lib/format";
import {
  cancelInvite,
  closeRoom,
  inviteToRoom,
  joinCrewRoom,
  leaveRoom,
  openCrewRoom,
  openMicSettings,
  renameRoom,
  toggleDeaf,
  toggleMute,
} from "./actions";
import { roomMenuItems } from "./crew-room-menu";
import { useCrewRoomsUi, withPendingOrder } from "./crew-rooms-store";
import { QualityBars, SpeakingFace } from "./faces";
import { HeadphonesSlash } from "./icons";
import { VoiceMeter } from "./meter";
import { meterThreshold, setOutputVolume, useVoicePrefs } from "./prefs";
import {
  crewRoomList,
  crewRoomName,
  invitables,
  keyLabel,
  micClosedLabel,
  NO_ROOM_POWERS,
  presenceOf,
  roleWord,
  roomFull,
  roomPowers,
  roomTitle,
  shownRoomName,
  type VoiceFace,
  voiceMark,
} from "./rules";
import { CallStage, isOneToOne } from "./stage";
import { useFace, useFaces, useVoiceUi } from "./store";

// --- marks beside a person --------------------------------------------------------------------

/** A state beside a person: its words (tooltip, the row's name) and how it is drawn. */
interface Mark {
  key: string;
  label: string;
  look: ReactNode;
}

const markIcon = "size-[15px]";

function marksOf(
  p: Pick<VoicePerson, "mute" | "deaf" | "mutedByModerator" | "quality"> & { mutedForMe?: boolean },
  t: (key: string) => string,
): Mark[] {
  const marks: Mark[] = [];
  // What is off, in one sign (voice.md §5.1; nothing while the microphone and the sound are on):
  // a moderator's mute in flare (it holds whatever they do), then deafened alone (the microphone is
  // off with it, as Discord shows it), else the microphone.
  const voice = voiceMark(p);
  if (p.mutedByModerator) {
    marks.push({
      key: "sm",
      label: t("voice.person.mutedForAll"),
      look: <MicrophoneSlash weight="bold" className={cn(markIcon, "text-flare-400")} />,
    });
  }
  if (voice === "deaf") {
    marks.push({
      key: "deaf",
      label: t("voice.person.deafened"),
      look: <HeadphonesSlash className={cn(markIcon, "text-ash-400")} />,
    });
  } else if (voice === "mute") {
    marks.push({
      key: "mute",
      label: t("voice.person.micOff"),
      look: <MicrophoneSlash weight="bold" className={cn(markIcon, "text-ash-400")} />,
    });
  }
  if (p.mutedForMe) {
    marks.push({
      key: "me",
      label: t("voice.person.mutedForMe"),
      look: <SpeakerSimpleSlash weight="bold" className={cn(markIcon, "text-ash-400")} />,
    });
  }
  if (p.quality !== "good") {
    marks.push({
      key: "q",
      label: t("voice.person.weak"),
      look: <QualityBars quality={p.quality} />,
    });
  }
  return marks;
}

function Marks({ marks }: { marks: Mark[] }) {
  return (
    <span className="flex shrink-0 items-center gap-1.5">
      {marks.map(({ key, label, look }) => (
        <Tooltip key={key} content={label}>
          <span aria-hidden className="flex">
            {look}
          </span>
        </Tooltip>
      ))}
    </span>
  );
}

// --- rows -------------------------------------------------------------------------------------

const rowShape =
  "flex w-full items-center gap-3 rounded-[var(--radius-panel)] py-1.5 pr-2 pl-2 text-left outline-none";

/** The name, then (SHARE-LIVE) "YAYINDA" right after it; the name truncates, the tag never does. */
function NameBlock({
  name,
  second,
  tag,
}: {
  name: string;
  second?: string | null;
  tag?: ReactNode;
}) {
  return (
    <span className="min-w-0 flex-1">
      <span className="flex min-w-0 items-center gap-[6px]">
        <span className="truncate text-[14px] leading-[1.25] font-semibold text-bone-50">
          {name}
        </span>
        {tag}
      </span>
      {second ? (
        <span className="block truncate text-[12px] leading-[1.35] text-ash-400">{second}</span>
      ) : null}
    </span>
  );
}

/** Where a row's card sits: beside the panel, level with the row (as a friend's card does). */
const ROW_CARD = { side: "left", align: "center", sideOffset: 14 } as const;

function MeRow({ room }: { room: VoiceRoomView }) {
  const { t } = useTranslation();
  const me = useSocialUi((s) => s.view.me);
  const rowRef = useRef<HTMLDivElement>(null);
  // SHARE-LIVE: the player's own stream, as the others see it.
  const live = useRoomTargets(room.me.id);
  const face: VoiceFace = {
    id: room.me.id,
    name: me?.name ?? t("voice.room.you"),
    avatar: me?.avatar,
    avatarUrl: me?.avatarUrl,
    friend: false,
  };
  // A role means something only with others in the room.
  const role = room.people.length ? roleWord(room.me.role, t) : null;
  const marks = marksOf(room.me, t);
  return (
    <div ref={rowRef} className={rowShape}>
      <SpeakingFace face={face} size={36} />
      <NameBlock
        name={face.name}
        second={role}
        tag={
          <LiveAppear show={live.length > 0}>
            <LiveTagButton targets={live} placement={{ ...ROW_CARD, anchor: rowRef }} />
          </LiveAppear>
        }
      />
      <Marks marks={marks} />
      {/* At the row's end, as the crew's member list marks the player. */}
      <span className="shrink-0 text-[12.5px] text-ash-400">{t("voice.room.you")}</span>
    </div>
  );
}

function PersonRow({ person }: { person: VoicePerson }) {
  const { t } = useTranslation();
  const face = useFace(person.id);
  const [open, setOpen] = useState(false);
  const rowRef = useRef<HTMLDivElement>(null);
  const ref = { id: face.id, name: face.name, avatar: face.avatar, avatarUrl: face.avatarUrl };
  // SHARE: "Yayını izle" first on someone live; a leader's or moderator's "Paylaşımı durdur" on
  // someone who shares.
  const items = [
    ...useWatchItem(person),
    ...usePersonItems(ref, { from: "list" }),
    ...useUnshareItem(person),
  ];
  // SHARE-LIVE: "YAYINDA" after the name; resting on it (or the row's keyboard focus) opens the
  // stream's card beside the panel.
  const live = useRoomTargets(person.id);
  const preview = useLivePreview(live, { ...ROW_CARD, anchor: rowRef });
  const kind = live[0]?.kind;
  const role = roleWord(person.role, t);
  const marks = marksOf(person, t);
  const label = [
    face.name,
    kind ? t("share.row.mark", { kind: t(`share.kind.${kind}`) }) : null,
    role,
    ...marks.map((m) => m.label),
  ]
    .filter(Boolean)
    .join(", ");
  return (
    // SHARE: "İzle" beside the row, outside its menu button.
    <div ref={rowRef} className="flex items-center gap-1">
      <Menu
        items={items}
        align="start"
        open={open}
        onOpenChange={setOpen}
        trigger={
          <button
            type="button"
            aria-label={label}
            {...preview.focusProps}
            // A right click, Shift+F10 or the menu key open the same menu (FIX2: one menu per person).
            onContextMenu={(event) => {
              event.preventDefault();
              setOpen(true);
            }}
            onKeyDown={(event) => {
              // SHARE-LIVE: → takes the keyboard into the stream's card.
              if (preview.keyIn(event)) return;
              if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
                event.preventDefault();
                setOpen(true);
              }
            }}
            aria-keyshortcuts={live.length ? "ArrowRight" : undefined}
            className={cn(
              rowShape,
              "min-w-0 flex-1",
              "cursor-pointer transition-[background-color,box-shadow] duration-150 hover:bg-white/[0.045] focus-visible:ring-2 focus-visible:ring-gold-300 data-[state=open]:bg-white/[0.06]",
            )}
          >
            <SpeakingFace face={face} size={36} />
            <NameBlock
              name={face.name}
              second={role}
              tag={
                <LiveAppear show={live.length > 0}>
                  <span {...preview.tagProps} data-live-tag className="flex shrink-0">
                    <LiveTag />
                  </span>
                </LiveAppear>
              }
            />
            <Marks marks={marks} />
          </button>
        }
      />
      <WatchButton person={person.id} name={face.name} className="shrink-0" />
      {preview.card}
    </div>
  );
}

function RingingRow({ id, onCancel }: { id: string; onCancel: () => void }) {
  const { t } = useTranslation();
  const face = useFace(id);
  return (
    <div className={rowShape}>
      <SpeakingFace face={face} size={36} dim speaking={false} />
      <NameBlock name={face.name} second={t("voice.room.ringing")} />
      <Button size="sm" variant="secondary" onClick={onCancel}>
        {t("voice.room.cancelRing")}
      </Button>
    </div>
  );
}

/**
 * Who is in the room: a call with one friend as the call's stage (VOICE-CALL), else the list. The
 * two cross-fade where they are when a third person comes or goes.
 */
function People({ room }: { room: VoiceRoomView }) {
  const { t } = useTranslation();
  const calling = useVoiceUi((s) => s.state.calling);
  const stage = isOneToOne({ room, calling });
  return (
    <AnimatePresence initial={false} mode="popLayout">
      <motion.div
        key={stage ? "stage" : "list"}
        // The call's stage sits in the middle of the panel, as a call does.
        className={stage ? "flex flex-1 flex-col" : undefined}
        initial={{ opacity: 0 }}
        animate={{ opacity: 1, transition: enter(0.22) }}
        exit={{ opacity: 0, transition: leave(0.14) }}
      >
        {stage ? (
          <CallStage room={room} marksOf={(person) => <Marks marks={marksOf(person, t)} />} />
        ) : (
          <PeopleList room={room} />
        )}
      </motion.div>
    </AnimatePresence>
  );
}

function PeopleList({ room }: { room: VoiceRoomView }) {
  const { reduce, layout } = useMotionPrefs();
  const calling = useVoiceUi((s) => s.state.calling);
  const rows: Array<{ key: string; node: ReactNode }> = [
    { key: "me", node: <MeRow room={room} /> },
    ...room.people.map((p) => ({ key: p.id, node: <PersonRow person={p} /> })),
    ...(calling
      ? [
          {
            key: `ring:${calling.friend}`,
            node: <RingingRow id={calling.friend} onCancel={() => void leaveRoom()} />,
          },
        ]
      : []),
    ...room.ringing.map((r) => ({
      key: `ring:${r.to}`,
      node: <RingingRow id={r.to} onCancel={() => void cancelInvite(r.to)} />,
    })),
  ];
  return (
    <ul className="relative flex flex-col gap-0.5">
      <AnimatePresence initial={false} mode="popLayout">
        {rows.map((row) => (
          <motion.li
            key={row.key}
            layout={layout ? "position" : false}
            transition={{ layout: glide }}
            // Layout glides compose with `y` (a transform string would fight them).
            initial={{ opacity: 0, y: reduce ? 0 : 6 }}
            animate={{ opacity: 1, y: 0, transition: enter(0.26) }}
            exit={{ opacity: 0, transition: leave(0.2) }}
          >
            {row.node}
          </motion.li>
        ))}
      </AnimatePresence>
    </ul>
  );
}

// --- what needs the player --------------------------------------------------------------------

function Notices({ room }: { room: VoiceRoomView }) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const mic = useVoiceUi((s) => s.state.mic);
  const settings = useVoicePrefs((s) => s.settings);
  const navigate = useNavigate();
  const closePanel = useVoiceUi((s) => s.closePanel);
  const notices: Array<{
    key: string;
    icon: ReactNode;
    text: string;
    action?: { label: string; onClick: () => void };
  }> = [];
  if (room.phase === "joining" || room.phase === "reconnecting") {
    notices.push({
      key: "connection",
      icon: <ArrowsClockwise size={16} weight="bold" className="text-gold-300" />,
      text: room.phase === "joining" ? t("voice.room.joining") : t("voice.room.reconnecting"),
    });
  }
  if (room.me.mutedByModerator) {
    notices.push({
      key: "sm",
      icon: <MicrophoneSlash size={16} weight="bold" className="text-flare-400" />,
      text: t("voice.mutedByMod"),
    });
  }
  // OVL-NEXT: a push-to-talk-only room. Without a key the microphone stays closed: the settings
  // give one. With one, a player who talks hands-free hears once which key opens it here.
  if (room.ptt && !room.me.mutedByModerator) {
    if (room.me.needsKey) {
      notices.push({
        key: "ptt",
        icon: <Warning size={16} weight="fill" className="text-gold-400" />,
        text: t("voice.ptt.needsKey"),
        action: {
          label: t("voice.ptt.pick"),
          onClick: () => {
            closePanel();
            void navigate({ to: "/settings", search: { section: "voice" } });
          },
        },
      });
    } else if (settings && settings.mode !== "pushToTalk" && settings.pushToTalkKey) {
      notices.push({
        key: "ptt",
        icon: <Keyboard size={16} weight="bold" className="text-ash-300" />,
        text: t("voice.ptt.hold", { key: keyLabel(settings.pushToTalkKey, t) }),
      });
    }
  }
  if (mic === "denied") {
    notices.push({
      key: "mic",
      icon: <Warning size={16} weight="fill" className="text-gold-400" />,
      text: t("voice.mic.denied"),
      action: { label: t("voice.mic.openSettings"), onClick: () => void openMicSettings() },
    });
  } else if (mic === "missing") {
    notices.push({
      key: "mic",
      icon: <Warning size={16} weight="fill" className="text-gold-400" />,
      text: t("voice.mic.missing"),
    });
  }
  return (
    <AnimatePresence initial={false} mode="popLayout">
      {notices.map((n) => (
        <motion.div
          key={n.key}
          className="mb-2 flex items-center gap-2.5 rounded-[var(--radius-panel)] bg-obsidian-950/40 px-3 py-2 ring-1 ring-white/7"
          {...rise(reduce, 4, 0.22)}
        >
          <span aria-hidden className="flex shrink-0">
            {n.icon}
          </span>
          <p className="min-w-0 flex-1 text-[13px] leading-snug text-bone-100" role="status">
            {n.text}
          </p>
          {n.action ? (
            <Button size="sm" variant="secondary" onClick={n.action.onClick}>
              {n.action.label}
            </Button>
          ) : null}
        </motion.div>
      ))}
    </AnimatePresence>
  );
}

// --- the header -------------------------------------------------------------------------------

function RoomName({
  room,
  title,
  editing,
  setEditing,
}: {
  room: VoiceRoomView;
  title: string;
  /** The header gives the field its room while the name is typed. */
  editing: boolean;
  setEditing: (editing: boolean) => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // Esc leaves the field, not the panel: the sheet listens on the document (capture), the field
  // on the window, which comes first (as SearchInput does).
  useEffect(() => {
    if (!editing) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      setEditing(false);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [editing, setEditing]);
  // Rights taken away while typing (the leader left) close the field.
  useEffect(() => {
    if (!room.canRename) setEditing(false);
  }, [room.canRename, setEditing]);

  // The field starts with the name as shown ("Room 2" for the service's "Oda 2").
  const shown = room.name ? shownRoomName(room.name, t) : "";
  const save = async () => {
    // Unchanged: nothing to send (an English "Room 2" would rename the crew's "Oda 2").
    if (draft.trim() === shown) {
      setEditing(false);
      return;
    }
    setBusy(true);
    const refused = await renameRoom(draft);
    setBusy(false);
    if (refused) setError(refused);
    else setEditing(false);
  };

  return (
    <div className="relative min-w-0 flex-1">
      <AnimatePresence initial={false} mode="popLayout">
        {editing ? (
          <motion.form
            key="edit"
            className="flex items-center gap-1.5"
            onSubmit={(event) => {
              event.preventDefault();
              void save();
            }}
            {...rise(reduce, 3, 0.2)}
          >
            <Dialog.Title className="sr-only">{title}</Dialog.Title>
            <Input
              autoFocus
              value={draft}
              maxLength={24}
              invalid={Boolean(error)}
              aria-label={t("voice.room.rename")}
              placeholder={t("voice.room.namePlaceholder")}
              onChange={(event) => {
                setDraft(event.target.value);
                setError(null);
              }}
              className="h-8 min-w-0 flex-1"
            />
            <Button size="sm" variant="ghost" type="button" onClick={() => setEditing(false)}>
              {t("voice.room.cancel")}
            </Button>
            <Button size="sm" variant="primary" type="submit" disabled={busy || !draft.trim()}>
              {t("voice.room.save")}
            </Button>
          </motion.form>
        ) : (
          <motion.div
            key="name"
            className="flex min-w-0 items-center gap-1"
            {...rise(reduce, 3, 0.2)}
          >
            <Dialog.Title className="truncate font-display text-[19px] leading-[1.2] font-semibold text-bone-50">
              {title}
            </Dialog.Title>
            {room.canRename ? (
              <IconButton
                size="sm"
                label={t("voice.room.rename")}
                icon={<PencilSimple size={15} />}
                onClick={() => {
                  setDraft(shown);
                  setError(null);
                  setEditing(true);
                }}
              />
            ) : null}
          </motion.div>
        )}
      </AnimatePresence>
      {editing && error ? (
        <p role="alert" className="mt-1 text-[12px] text-flare-300">
          {error}
        </p>
      ) : null}
    </div>
  );
}

/**
 * The crew's rooms as tabs, in the crew page's order and names ("Genel" first; VOICE-ROOMS: a right
 * click or Shift+F10 opens the room's menu: its own action and, by role, its place; changing and
 * deleting rooms stay on the crew page, where the row takes the form).
 */
function CrewRooms({ room }: { room: VoiceRoomView }) {
  const { t } = useTranslation();
  const crewRooms = useVoiceUi((s) => s.state.crews.find((c) => c.crew === room.crew));
  const team = useSocialUi((s) => s.view.crews.find((c) => c.id === room.crew));
  const hidden = useCrewRoomsUi((s) => s.hidden);
  const pending = useCrewRoomsUi((s) => (room.crew ? s.order[room.crew] : undefined));
  if (!room.crew) return null;
  const crew = room.crew;
  const [general, ...rest] = crewRoomList(crew, crewRooms, hidden);
  const ordered = [...(general ? [general] : []), ...withPendingOrder(rest, pending)];
  // The player's own room is always there, even before the crew's list says so.
  const rooms = ordered.some((r) => r.id === room.id)
    ? ordered
    : [
        ...ordered,
        {
          id: room.id,
          name: room.name ?? "",
          general: room.general,
          people: [],
          max: null,
          own: false,
        },
      ];
  const manage = crewRooms?.manage ?? false;
  const standing = team ? { role: team.role, moderator: team.moderator } : null;
  const order = rooms.filter((r) => !r.general).map((r) => r.id);
  return (
    <fieldset className="-mx-1 mt-3 flex min-w-0 gap-1.5 overflow-x-auto border-0 px-1 pb-0.5 [scrollbar-width:none]">
      <legend className="sr-only">{t("voice.room.rooms")}</legend>
      {rooms.map((r) => {
        const here = r.id === room.id;
        const count = here ? room.people.length + 1 : r.people.length;
        const shown = crewRoomName(r, t);
        const powers = standing ? roomPowers(standing, r, manage) : NO_ROOM_POWERS;
        const items = roomMenuItems(
          {
            crew,
            room: r,
            here,
            powers: { ...powers, edit: false, remove: false },
            order,
            across: true,
            onEdit: () => {},
            onRemove: () => {},
          },
          t,
        );
        return (
          <ContextMenu
            key={r.id}
            items={items}
            title={shown}
            wrapTrigger={(trigger) => (
              <Tooltip content={t("voice.crew.join")} disabled={here}>
                {trigger}
              </Tooltip>
            )}
          >
            <button
              type="button"
              aria-current={here ? "true" : undefined}
              aria-label={`${shown}, ${t("voice.crew.people", { count })}`}
              onClick={here ? undefined : () => void joinCrewRoom(crew, r.id)}
              onKeyDown={(event) => void openMenuFromKey(event)}
              className={cn(
                "chamfer flex h-7 shrink-0 items-center gap-1.5 px-3 text-[12.5px] font-medium whitespace-nowrap outline-none [--cut:6px]",
                "transition-[background-color,color,transform,scale] duration-150 ease-out focus-visible:ring-2 focus-visible:ring-gold-300",
                here
                  ? "cursor-default bg-white/14 text-bone-50"
                  : "cursor-pointer bg-white/6 text-ash-300 hover:bg-white/10 hover:text-bone-50 active:scale-[0.97]",
              )}
            >
              {shown}
              {/* A person's glyph before the count: "Oda 2 1" read as "Oda 21". */}
              <span aria-hidden className="flex items-center gap-0.5 text-ash-300">
                <User size={11} weight="bold" />
                <span className="font-display text-[12px] font-semibold tabular-nums">{count}</span>
              </span>
            </button>
          </ContextMenu>
        );
      })}
      {crewRooms?.canAdd ? (
        <Tooltip content={t("voice.room.newRoom")}>
          <button
            type="button"
            aria-label={t("voice.room.newRoom")}
            onClick={() => void openCrewRoom(crew)}
            className="chamfer flex size-7 shrink-0 cursor-pointer items-center justify-center bg-white/6 text-ash-300 outline-none transition-[background-color,color,transform,scale] duration-150 ease-out [--cut:6px] hover:bg-white/10 hover:text-bone-50 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.94]"
          >
            <Plus size={14} weight="bold" aria-hidden />
          </button>
        </Tooltip>
      ) : null}
    </fieldset>
  );
}

function RoomHeader({ room, title }: { room: VoiceRoomView; title: string }) {
  const { t } = useTranslation();
  const goTo = useVoiceUi((s) => s.goTo);
  const close = useVoiceUi((s) => s.closePanel);
  const navigate = useNavigate();
  const crew = useSocialUi((s) => s.view.crews.find((c) => c.id === room.crew));
  // While the name is typed, the field takes the header's room ("Davet et" and the settings wait).
  const [editing, setEditing] = useState(false);
  const full = roomFull(room);
  const count = room.people.length + 1;
  // VOICE-CALL: the call's stage says what it is ("1 kişi" read wrong above two faces while ringing).
  const stage = useVoiceUi((s) => isOneToOne(s.state));
  return (
    <header className="shrink-0 border-b border-white/8 pt-4 pr-3 pb-3 pl-5">
      <div className="flex items-start gap-1.5">
        <div className="min-w-0 flex-1 pt-0.5">
          <RoomName room={room} title={title} editing={editing} setEditing={setEditing} />
          <p className="mt-0.5 truncate text-[12.5px] text-ash-400">
            {room.kind === "crew" && crew
              ? t("voice.room.crewOf", { crew: crew.name })
              : stage
                ? t("voice.call.kind")
                : t("voice.room.count", { count })}
          </p>
        </div>
        {editing ? null : (
          <>
            <Tooltip content={t("voice.refused.roomFull")} disabled={!full}>
              <Button
                size="sm"
                variant="secondary"
                icon={<UserPlus size={15} />}
                aria-disabled={full || undefined}
                onClick={full ? undefined : () => goTo("invite")}
                className={cn("mt-0.5", full && "opacity-45")}
              >
                {t("voice.room.invite")}
              </Button>
            </Tooltip>
            <IconButton
              label={t("voice.room.settings")}
              icon={<GearSix size={18} />}
              onClick={() => {
                close();
                void navigate({ to: "/settings", search: { section: "voice" } });
              }}
            />
          </>
        )}
        <Dialog.Close asChild>
          <IconButton label={t("common.close")} icon={<X size={18} />} />
        </Dialog.Close>
      </div>
      {room.kind === "crew" ? <CrewRooms room={room} /> : null}
    </header>
  );
}

// --- the bottom -------------------------------------------------------------------------------

function CloseRoom() {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  return (
    <div className="relative mt-2 h-8">
      <AnimatePresence initial={false} mode="popLayout">
        {asking ? (
          <motion.div key="ask" className="flex h-8 items-center gap-2" {...rise(reduce, 4, 0.2)}>
            <p className="min-w-0 flex-1 truncate text-[13px] text-bone-100">
              {t("voice.room.closeAsk")}
            </p>
            <Button size="sm" variant="ghost" onClick={() => setAsking(false)}>
              {t("voice.room.cancel")}
            </Button>
            <Button
              size="sm"
              variant="danger"
              disabled={busy}
              onClick={async () => {
                setBusy(true);
                await closeRoom();
                setBusy(false);
                setAsking(false);
              }}
            >
              {t("voice.room.close")}
            </Button>
          </motion.div>
        ) : (
          <motion.div key="idle" className="flex h-8 justify-center" {...rise(reduce, 4, 0.2)}>
            <Button size="sm" variant="ghost" onClick={() => setAsking(true)}>
              {/* The words carry the flare (a ghost button colours its own label). */}
              <span className="text-flare-400 transition-colors group-hover:text-flare-300">
                {t("voice.room.close")}
              </span>
            </Button>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

/**
 * "Herkesin sesi" (VOICE-CALL, SOC-GAP §3.11): everyone's voice at once, the person menu's slider
 * (its notch at 100 %, to the engine's top). A right click on "Sağırlaştır" opens it, as Discord's
 * headphones do; Settings → Sesli sohbet has it too.
 */
function useOutputVolumeItem(): MenuItem[] {
  const { t } = useTranslation();
  const settings = useVoicePrefs((s) => s.settings);
  const max = useVoiceUi((s) => s.state.volumeMax ?? 200);
  if (!settings) return [];
  return [
    {
      label: t("voice.level.output"),
      icon: <SpeakerHigh size={16} />,
      onSelect: () => {},
      slider: {
        value: Math.min(settings.outputVolume, max),
        min: 0,
        max,
        step: 5,
        notch: max > 100 ? 100 : undefined,
        format: formatPercent,
        onChange: setOutputVolume,
      },
    },
  ];
}

function RoomFooter({ room }: { room: VoiceRoomView }) {
  const { t } = useTranslation();
  const { me } = room;
  // OVL-NEXT: no push-to-talk key in a push-to-talk-only room keeps it closed too.
  const closed = micClosedLabel(me, t);
  const micOff = me.mute || me.deaf || closed !== null;
  const micLabel = closed ?? (micOff ? t("voice.room.unmuteMic") : t("voice.room.muteMic"));
  const deafLabel = me.deaf ? t("voice.room.deafenOff") : t("voice.room.deafenOn");
  const volume = useOutputVolumeItem();
  const threshold = useVoicePrefs((s) => meterThreshold(s.settings));
  // VOICE-CALL: on the call's stage the footer is its only way out ("İptal" while it rings) and a
  // call of two has nothing to close for the other (leaving is hanging up, as on Discord).
  const stage = useVoiceUi((s) => isOneToOne(s.state));
  const ringing = useVoiceUi((s) => Boolean(s.state.calling)) && stage;
  return (
    <footer className="shrink-0 border-t border-white/8 px-4 pt-3 pb-3.5">
      {/* SHARE: the player's shares and "Ekranını paylaş". */}
      <ShareFooterRow />
      {/* "Mikrofon" a little wider: its meter beside the word ("Microphone" in English). */}
      <div className="grid grid-cols-[1.3fr_1fr_1fr] gap-2">
        <Tooltip content={micLabel}>
          <Button
            size="lg"
            variant="secondary"
            aria-label={micLabel}
            aria-disabled={closed !== null || undefined}
            className={cn("w-full px-2", closed !== null && "opacity-60")}
            icon={
              micOff ? (
                <MicrophoneSlash size={17} weight="bold" className="text-flare-400" />
              ) : (
                <Microphone size={17} weight="bold" />
              )
            }
            // What the microphone hears, beside its word (VOICE-CALL); still while it is off.
            iconRight={
              <VoiceMeter threshold={threshold} className={cn("ml-0.5", micOff && "opacity-60")} />
            }
            onClick={closed !== null ? undefined : () => void toggleMute()}
          >
            {t("voice.room.mic")}
          </Button>
        </Tooltip>
        <ContextMenu
          items={volume}
          disabled={!volume.length}
          wrapTrigger={(trigger) => <Tooltip content={deafLabel}>{trigger}</Tooltip>}
        >
          <Button
            size="lg"
            variant="secondary"
            aria-label={deafLabel}
            className="w-full px-2"
            icon={
              me.deaf ? (
                <HeadphonesSlash size={17} className="text-flare-400" />
              ) : (
                <Headphones size={17} weight="bold" />
              )
            }
            onClick={() => void toggleDeaf()}
          >
            {t("voice.room.deafen")}
          </Button>
        </ContextMenu>
        <Button
          size="lg"
          variant="secondary"
          className="w-full px-2"
          // Leaving ends nothing for the others: a plate with a red glyph; "Odayı kapat" alone is red.
          icon={<PhoneDisconnect size={17} weight="bold" className="text-flare-400" />}
          aria-label={ringing ? t("voice.call.cancelLabel") : undefined}
          onClick={() => void leaveRoom()}
        >
          {ringing ? t("voice.room.cancelRing") : t("voice.room.leave")}
        </Button>
      </div>
      {/* Alone, closing is leaving: "Ayrıl" says it. */}
      {room.canClose && room.people.length > 0 && !stage ? <CloseRoom /> : null}
    </footer>
  );
}

// --- the pages --------------------------------------------------------------------------------

function RoomPage({ room }: { room: VoiceRoomView }) {
  const { t } = useTranslation();
  const calling = useVoiceUi((s) => s.state.calling);
  const faces = useFaces([...room.people.map((p) => p.id), ...(calling ? [calling.friend] : [])]);
  const nameOf = (id: string) => faces.find((f) => f.id === id)?.name ?? t("voice.someone");
  const title = roomTitle(room, nameOf, t, calling?.friend);
  return (
    <>
      <RoomHeader room={room} title={title} />
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain px-3 pt-3 pb-4">
        <Notices room={room} />
        <People room={room} />
      </div>
      <RoomFooter room={room} />
    </>
  );
}

function InviteRow({ person, full }: { person: VoiceFace & { ringing: boolean }; full: boolean }) {
  const { t } = useTranslation();
  const friends = useSocialUi((s) => s.view.friends);
  const presence = useVoiceUi((s) => presenceOf(person.id, s.state, friends));
  return (
    <li className="flex items-center gap-3 rounded-[var(--radius-panel)] py-1.5 pr-1 pl-2 transition-colors duration-150 hover:bg-white/[0.045]">
      <SocialFace
        source={person}
        size={36}
        className="overflow-hidden rounded-[var(--radius-head)]"
      />
      <NameBlock
        name={person.name}
        second={presence === "in" ? t("voice.presence.inVoice") : null}
      />
      <Button
        size="sm"
        variant="secondary"
        disabled={person.ringing || full}
        onClick={() => void inviteToRoom(person.id)}
      >
        <SwapLabel
          labels={[t("voice.invite.action"), t("voice.invite.ringing")]}
          active={person.ringing ? 1 : 0}
        />
      </Button>
    </li>
  );
}

function InvitePage({ room }: { room: VoiceRoomView }) {
  const { t } = useTranslation();
  const goTo = useVoiceUi((s) => s.goTo);
  const state = useVoiceUi((s) => s.state);
  const friends = useSocialUi((s) => s.view.friends);
  const crews = useSocialUi((s) => s.view.crews);
  const [query, setQuery] = useState("");
  const list = invitables(state, { friends, crews }, query);
  const full = roomFull(room);
  const crewRoom = room.kind === "crew";
  return (
    <>
      <header className="flex h-[60px] shrink-0 items-center gap-2 border-b border-white/8 pr-3 pl-3">
        <IconButton
          label={t("voice.invite.back")}
          icon={<CaretLeft size={18} weight="bold" />}
          onClick={() => goTo("room")}
        />
        <Dialog.Title className="min-w-0 flex-1 truncate font-display text-[19px] leading-tight font-semibold text-bone-50">
          {t("voice.invite.title")}
        </Dialog.Title>
        <Dialog.Close asChild>
          <IconButton label={t("common.close")} icon={<X size={18} />} />
        </Dialog.Close>
      </header>
      <div className="shrink-0 px-4 pt-3">
        <SearchInput
          autoFocus
          value={query}
          onChange={setQuery}
          label={crewRoom ? t("voice.invite.searchCrew") : t("voice.invite.searchFriends")}
          placeholder={crewRoom ? t("voice.invite.searchCrew") : t("voice.invite.searchFriends")}
        />
      </div>
      {list.length ? (
        <ul className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 pt-2 pb-6">
          {list.map((person) => (
            <InviteRow key={person.id} person={person} full={full} />
          ))}
        </ul>
      ) : (
        <p className="px-5 pt-5 text-[13px] text-ash-400">
          {query.trim() ? t("voice.invite.noMatch") : t("voice.invite.none")}
        </p>
      )}
    </>
  );
}

// --- the sheet --------------------------------------------------------------------------------

export function RoomSheet() {
  const panel = useVoiceUi((s) => s.panel);
  const close = useVoiceUi((s) => s.closePanel);
  const room = useVoiceUi((s) => s.state.room);
  const page = useVoiceUi((s) => s.page);
  const direction = useVoiceUi((s) => s.direction);
  const returnFocus = useVoiceUi((s) => s.returnFocus);
  const animate = useInteractionMotion();
  const { reduce } = useMotionPrefs();
  // The sheet keeps the last room while it slides away.
  const last = useRef<VoiceRoomView | null>(null);
  if (room) last.current = room;
  const shown = room ?? last.current;
  const open = panel && room !== null;
  const variants = pageVariants(reduce);
  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && close()}>
      <AnimatePresence>
        {open && shown ? (
          <EntranceGate key="sheet">
            <Dialog.Portal forceMount>
              <Dialog.Overlay asChild forceMount>
                <GatedMotion.div
                  className="fixed inset-0 z-30 bg-obsidian-950/55"
                  {...scrimMotion(!animate)}
                />
              </Dialog.Overlay>
              <Dialog.Content
                asChild
                forceMount
                aria-describedby={undefined}
                onOpenAutoFocus={(event) => {
                  event.preventDefault();
                  focusPanel(event.currentTarget as HTMLElement);
                }}
                onCloseAutoFocus={(event) => {
                  if (returnFocus?.isConnected) {
                    event.preventDefault();
                    returnFocus.focus();
                  }
                }}
                // A press outside closes it; answering a toast (a ring) does not.
                {...sheetOutside}
              >
                <GatedMotion.aside
                  tabIndex={-1}
                  className="glass-strong fixed top-0 right-0 bottom-0 z-40 flex w-[min(440px,94vw)] flex-col overflow-hidden shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none"
                  {...sheetMotion(!animate)}
                >
                  <ReducedMotionConfig>
                    <AnimatePresence mode="popLayout" initial={false} custom={direction}>
                      <motion.div
                        key={page}
                        custom={direction}
                        variants={variants}
                        initial="enter"
                        animate="center"
                        exit="exit"
                        className="flex min-h-0 flex-1 flex-col"
                      >
                        {page === "invite" ? (
                          <InvitePage room={shown} />
                        ) : (
                          <RoomPage room={shown} />
                        )}
                      </motion.div>
                    </AnimatePresence>
                  </ReducedMotionConfig>
                </GatedMotion.aside>
              </Dialog.Content>
            </Dialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}
