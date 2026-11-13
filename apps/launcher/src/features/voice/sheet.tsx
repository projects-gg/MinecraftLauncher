// The room panel (VOICE-L3; voice.md §5.1): a sheet from the right, as the friends panel is. The
// header names the room (its leader renames it in place) with "Davet et", the settings and close;
// a crew room shows the crew's rooms under it as chips with their counts ("Genel 4", "Oda 2 1"),
// another room's chip joins it and "+" opens a new one. Then what needs the player (the microphone,
// a moderator's mute, the connection), the people (the player first, then the others as they came,
// then invites that ring with "İptal"), each with their speaking ring, role word and state marks;
// a press or a right click opens their menu (volume first). At the bottom "Mikrofon",
// "Sağırlaştır" and "Ayrıl", and for the leader "Odayı kapat" with its one sentence. "Davet et"
// slides in over the room as the friends panel's pages do.
//
// SQOL-U3 (the owner: "sesli sohbete girince orada bir metin sohbet alanı daha olsun"): with a chat
// the panel reads header, the people (compact under "Odadakiler · 5", folding away, at most ~45 %
// of the panel with their own scroll; a call with one friend keeps its two faces, smaller), the chat
// in the rest (room-chat/panel-chat.tsx), then the bottom; without one the people take the room as
// before. While the stream's stage is on the page the panel docks beside it instead of over it: no
// scrim, the page stays live, a press outside leaves it, "Ses Odası" as a landmark; X and (focus
// inside) Esc close it, and the stage leaves its width free (`dock`). It slides in on its own; a
// stage that comes or goes while it shows changes its shell in place (no slide again, the field
// keeps the focus it had). A stage over the whole window hides it until it is back.
import {
  ArrowsClockwise,
  CaretDown,
  CaretLeft,
  DotsThree,
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
  XCircle,
} from "@phosphor-icons/react";
import {
  Dialog as AskDialog,
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
import {
  Fragment,
  type KeyboardEvent,
  memo,
  type ReactNode,
  useEffect,
  useId,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { useRoomTargets } from "@/features/share/live";
import { LiveAppear, LiveTag, LiveTagButton, useLivePreview } from "@/features/share/live-card";
import { useUnshareItem, useWatchItem } from "@/features/share/menu";
import { ShareFooterRow, useShareRuleItems, WatchButton } from "@/features/share/panel";
import { openMenuFromKey } from "@/features/social/menu-key";
import { enter, glide, leave, pageVariants, rise } from "@/features/social/motion";
import { usePersonItems } from "@/features/social/person-menu";
import { SocialFace } from "@/features/social/social-face";
import { useSocialUi } from "@/features/social/store";
import type { VoicePerson, VoiceQuality, VoiceRoomView } from "@/lib/api/voice-types";
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
import { ConnectionMark } from "./connection";
import { roomMenuItems } from "./crew-room-menu";
import { useCrewRoomsUi, withPendingOrder } from "./crew-rooms-store";
import {
  type Arrival,
  arrivalOf,
  FRESH,
  useToastsBeside,
  useWindowTaller,
  useWindowWidth,
} from "./dock";
import { SpeakingFace } from "./faces";
import { FittedNames } from "./fitted-names";
import { HeadphonesSlash } from "./icons";
import { VoiceMeter } from "./meter";
import {
  type PanelChrome,
  PanelChromeProvider,
  PanelClose,
  PanelTitle,
  usePanelChrome,
} from "./panel-chrome";
import { PanelResizer } from "./panel-resize";
import {
  closeInMenu,
  connectionWords,
  footerActions,
  headerLine,
  panelTitle,
  renameBesideTitle,
  renameInMenu,
  titleNames,
} from "./panel-rules";
import {
  clampWidth,
  layoutFor,
  type PanelPlace,
  usePanelSize,
  type WidthLimits,
  widthLimits,
} from "./panel-size";
import { meterThreshold, setOutputVolume, useVoicePrefs } from "./prefs";
import { PanelChatSection, usePanelChat } from "./room-chat";
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
  shownRoomName,
  type VoiceFace,
  voiceMark,
} from "./rules";
import { useSpeakingWord } from "./speaking-word";
import { CallStage, isOneToOne } from "./stage";
import { useStageMode } from "./stage-mode";
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
  p: Pick<VoicePerson, "mute" | "deaf" | "mutedByModerator"> & { mutedForMe?: boolean },
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
  return marks;
}

/**
 * The marks, and last (SQOL-U3) a connection that is not good, as a keyboard stop of its own
 * (`ConnectionMark`): never inside a row's button, which draws it beside itself.
 */
function Marks({ marks, connection }: { marks: Mark[]; connection?: VoiceQuality }) {
  return (
    <span className="flex shrink-0 items-center gap-1.5">
      {marks.map(({ key, label, look }) => (
        <Tooltip key={key} content={label}>
          <span aria-hidden className="flex">
            {look}
          </span>
        </Tooltip>
      ))}
      {connection ? <ConnectionMark quality={connection} /> : null}
    </span>
  );
}

// --- rows -------------------------------------------------------------------------------------

const rowShape =
  "flex w-full items-center gap-3 rounded-[var(--radius-panel)] py-1.5 pr-2 pl-2 text-left outline-none";
/** SQOL-U3: with a chat under them the rows are closer, their faces 32 px (whole pixels). */
const compactRowShape =
  "flex w-full items-center gap-2.5 rounded-[var(--radius-panel)] py-1 pr-2 pl-2 text-left outline-none";

/** A row's face: 36 px, or 32 px over a chat. */
const faceSize = (compact: boolean) => (compact ? 32 : 36);

/** The name, then (SHARE-LIVE) "YAYINDA" right after it; the name truncates, the tag never does. */
function NameBlock({
  name,
  second,
  tag,
  compact = false,
}: {
  name: string;
  second?: string | null;
  tag?: ReactNode;
  compact?: boolean;
}) {
  return (
    <span className="min-w-0 flex-1">
      <span className="flex min-w-0 items-center gap-[6px]">
        <span
          className={cn(
            "truncate leading-[1.25] font-semibold text-bone-50",
            compact ? "text-[13.5px]" : "text-[14px]",
          )}
        >
          {name}
        </span>
        {tag}
      </span>
      {second ? (
        <span
          className={cn(
            "block truncate leading-[1.35] text-ash-400",
            compact ? "text-[11.5px]" : "text-[12px]",
          )}
        >
          {second}
        </span>
      ) : null}
    </span>
  );
}

/** Where a row's card sits: beside the panel, level with the row (as a friend's card does). */
const ROW_CARD = { side: "left", align: "center", sideOffset: 14 } as const;

function MeRow({ room, compact }: { room: VoiceRoomView; compact: boolean }) {
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
    <div ref={rowRef} className={compact ? compactRowShape : rowShape}>
      <SpeakingFace face={face} size={faceSize(compact)} />
      <NameBlock
        name={face.name}
        second={role}
        compact={compact}
        tag={
          <LiveAppear show={live.length > 0}>
            <LiveTagButton targets={live} placement={{ ...ROW_CARD, anchor: rowRef }} />
          </LiveAppear>
        }
      />
      <Marks marks={marks} connection={room.me.quality} />
      {/* SQOL-U3 (the owner: everyone knows who they are): "Sen" only to screen readers. */}
      <span className="sr-only">{t("voice.room.you")}</span>
    </div>
  );
}

function PersonRow({ person, compact }: { person: VoicePerson; compact: boolean }) {
  const { t } = useTranslation();
  const face = useFace(person.id);
  const [open, setOpen] = useState(false);
  const rowRef = useRef<HTMLDivElement>(null);
  // SQOL-U3: the row's name says "konuşuyor" while they speak (after a moment, for a calm reader).
  const speaking = useSpeakingWord(person.id);
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
    speaking ? t("roomChat.speaking") : null,
    kind ? t("share.row.mark", { kind: t(`share.kind.${kind}`) }) : null,
    role,
    ...marks.map((m) => m.label),
    person.quality === "good" ? null : connectionWords(person.quality, t),
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
              compact ? compactRowShape : rowShape,
              "min-w-0 flex-1",
              "cursor-pointer transition-[background-color,box-shadow] duration-150 hover:bg-white/[0.045] focus-visible:ring-2 focus-visible:ring-gold-300 data-[state=open]:bg-white/[0.06]",
            )}
          >
            <SpeakingFace face={face} size={faceSize(compact)} />
            <NameBlock
              name={face.name}
              second={role}
              compact={compact}
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
      {/* SQOL-U3: beside the row's button, a stop of its own (its words by keyboard too). */}
      <ConnectionMark quality={person.quality} className="mr-1" />
      <WatchButton person={person.id} name={face.name} className="shrink-0" />
      {preview.card}
    </div>
  );
}

function RingingRow({
  id,
  onCancel,
  compact,
}: {
  id: string;
  onCancel: () => void;
  compact: boolean;
}) {
  const { t } = useTranslation();
  const face = useFace(id);
  return (
    <div className={compact ? compactRowShape : rowShape}>
      <SpeakingFace face={face} size={faceSize(compact)} dim speaking={false} />
      <NameBlock name={face.name} second={t("voice.room.ringing")} compact={compact} />
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
          <CallStage
            room={room}
            marksOf={(person) => <Marks marks={marksOf(person, t)} connection={person.quality} />}
          />
        ) : (
          <PeopleList room={room} compact={false} />
        )}
      </motion.div>
    </AnimatePresence>
  );
}

function PeopleList({ room, compact }: { room: VoiceRoomView; compact: boolean }) {
  const { reduce, layout } = useMotionPrefs();
  const calling = useVoiceUi((s) => s.state.calling);
  const rows: Array<{ key: string; node: ReactNode }> = [
    { key: "me", node: <MeRow room={room} compact={compact} /> },
    ...room.people.map((p) => ({ key: p.id, node: <PersonRow person={p} compact={compact} /> })),
    ...(calling
      ? [
          {
            key: `ring:${calling.friend}`,
            node: (
              <RingingRow id={calling.friend} onCancel={() => void leaveRoom()} compact={compact} />
            ),
          },
        ]
      : []),
    ...room.ringing.map((r) => ({
      key: `ring:${r.to}`,
      node: <RingingRow id={r.to} onCancel={() => void cancelInvite(r.to)} compact={compact} />,
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

/**
 * SQOL-U3: the people over a chat. A call with one friend keeps its two faces (smaller, a band at
 * the top); otherwise "Odadakiler · 5" folds the list away (the player's choice, kept while the
 * launcher runs) and the list scrolls on its own within ~45 % of the panel. Until the player
 * chooses, a short panel starts folded (beside a stream under 800 px of window, over the page under
 * 700): the chat would have room for a message or two.
 */
function PeopleSection({ room }: { room: VoiceRoomView }) {
  const { t } = useTranslation();
  const { docked } = usePanelChrome();
  const calling = useVoiceUi((s) => s.state.calling);
  const chosen = useVoiceUi((s) => s.peopleOpen);
  const setOpen = useVoiceUi((s) => s.setPeopleOpen);
  const tall = useWindowTaller(docked ? 800 : 700);
  const open = chosen ?? tall;
  const listId = useId();
  if (isOneToOne({ room, calling })) {
    return (
      <div className="shrink-0 border-b border-white/8">
        <CallStage
          room={room}
          compact
          marksOf={(person) => <Marks marks={marksOf(person, t)} connection={person.quality} />}
        />
      </div>
    );
  }
  const count = room.people.length + 1;
  return (
    <section className="flex max-h-[45%] min-h-0 shrink-0 flex-col border-b border-white/8">
      <h3 className="shrink-0">
        <button
          type="button"
          aria-expanded={open}
          aria-controls={open ? listId : undefined}
          aria-label={t("roomChat.people.label", { count })}
          onClick={() => setOpen(!open)}
          className="flex h-9 w-full cursor-pointer items-center gap-1.5 pr-4 pl-5 text-left text-[12.5px] font-semibold text-ash-300 outline-none transition-colors duration-150 hover:text-bone-50 focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:ring-inset"
        >
          <span>{t("roomChat.people.title")}</span>
          <span aria-hidden className="text-ash-500">
            ·
          </span>
          <span className="tabular-nums">{count}</span>
          <CaretDown
            size={13}
            weight="bold"
            aria-hidden
            className={cn(
              "ml-auto text-ash-400 transition-transform duration-150 ease-out",
              !open && "-rotate-90",
            )}
          />
        </button>
      </h3>
      {open ? (
        // A soft bottom edge: a row cut there reads as "more below".
        <div
          id={listId}
          className="min-h-0 overflow-y-auto overscroll-contain px-3 pb-3 [mask-image:linear-gradient(to_top,transparent,black_14px)]"
        >
          <PeopleList room={room} compact />
        </div>
      ) : null}
    </section>
  );
}

/**
 * SQOL-U3: the people beside the chat in a wide panel: a column of their own, the whole list (no
 * fold), the rows' full faces with their speaking rings and live marks; a call with one friend, its
 * two faces one over the other in the column's middle.
 */
function PeopleColumn({ room, wider }: { room: VoiceRoomView; wider: boolean }) {
  const { t } = useTranslation();
  const calling = useVoiceUi((s) => s.state.calling);
  const titleId = useId();
  const count = room.people.length + 1;
  return (
    <div
      className={cn(
        "flex min-h-0 shrink-0 flex-col border-r border-white/8",
        wider ? "w-[300px]" : "w-[260px]",
      )}
    >
      <div className="shrink-0 px-3 pt-3 empty:hidden">
        <Notices room={room} />
      </div>
      {isOneToOne({ room, calling }) ? (
        <CallStage
          room={room}
          stacked
          marksOf={(person) => <Marks marks={marksOf(person, t)} connection={person.quality} />}
        />
      ) : (
        <section aria-labelledby={titleId} className="flex min-h-0 flex-1 flex-col">
          <h3
            id={titleId}
            aria-label={t("roomChat.people.label", { count })}
            className="flex h-9 shrink-0 items-center gap-1.5 pr-4 pl-5 text-[12.5px] font-semibold text-ash-300"
          >
            <span>{t("roomChat.people.title")}</span>
            <span aria-hidden className="text-ash-500">
              ·
            </span>
            <span className="tabular-nums">{count}</span>
          </h3>
          {/* A soft bottom edge: a row cut there reads as "more below". */}
          <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-3 pb-3 [mask-image:linear-gradient(to_top,transparent,black_14px)]">
            <PeopleList room={room} compact={false} />
          </div>
        </section>
      )}
    </div>
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
  names,
  editing,
  setEditing,
}: {
  room: VoiceRoomView;
  title: string;
  /** SQOL-U3: an unnamed room's people, fitted to the title's line at a name's end. */
  names: readonly string[] | null;
  /** The header gives the field its room while the name is typed. */
  editing: boolean;
  setEditing: (editing: boolean) => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const [draft, setDraft] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const row = useRef<HTMLDivElement>(null);
  const pencil = useRef<HTMLButtonElement>(null);
  // The title's room: its row less the pencil and the gap before it (`gap-1`).
  const titleRoom = () => {
    const box = row.current;
    return box ? box.clientWidth - (pencil.current ? pencil.current.offsetWidth + 4 : 0) : null;
  };
  // Esc leaves the field, not the panel: the sheet listens on the document (capture), the field
  // on the window, which comes first (as SearchInput does).
  useEffect(() => {
    if (!editing) return;
    const onKey = (event: globalThis.KeyboardEvent) => {
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

  // The field starts with the name as shown ("Room 2" for the service's "Oda 2"), whichever way
  // it opened (the pencil, a crew room's menu).
  const shown = room.name ? shownRoomName(room.name, t) : "";
  // biome-ignore lint/correctness/useExhaustiveDependencies: only as the field opens (a rename by someone else while typing keeps the draft)
  useLayoutEffect(() => {
    if (!editing) return;
    setDraft(shown);
    setError(null);
  }, [editing]);
  // SQOL-U3: a crew's title is the crew's name: its rooms are renamed from the header's menu.
  const pencilShown = renameBesideTitle(room);
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
            <PanelTitle className="sr-only">{title}</PanelTitle>
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
            ref={row}
            className="flex min-w-0 items-center gap-1"
            {...rise(reduce, 3, 0.2)}
          >
            <PanelTitle className="truncate font-display text-[19px] leading-[1.2] font-semibold text-bone-50">
              {names ? (
                <FittedNames names={names} room={titleRoom} box={row} refit={pencilShown} />
              ) : (
                title
              )}
            </PanelTitle>
            {pencilShown ? (
              <IconButton
                ref={pencil}
                size="sm"
                label={t("voice.room.rename")}
                icon={<PencilSimple size={15} />}
                onClick={() => setEditing(true)}
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

function RoomHeader({
  room,
  title,
  names,
  counted = false,
}: {
  room: VoiceRoomView;
  title: string;
  names: readonly string[] | null;
  /** SQOL-U3: the people's own line counts them ("Odadakiler · 5"): the header does not again. */
  counted?: boolean;
}) {
  const { t } = useTranslation();
  const goTo = useVoiceUi((s) => s.goTo);
  // While the name is typed, the field takes the header's room ("Davet et" and the menu wait).
  const [editing, setEditing] = useState(false);
  const full = roomFull(room);
  // VOICE-CALL: the call's stage says what it is ("1 kişi" read wrong above two faces while ringing).
  const stage = useVoiceUi((s) => isOneToOne(s.state));
  // SQOL-U3: a crew's chips and a chat's people line count the people; the header then says only
  // a full room.
  const second = headerLine(room, { stage, counted, full }, t);
  return (
    <header className="shrink-0 border-b border-white/8 pt-4 pr-3 pb-3 pl-5">
      <div className="flex items-start gap-1.5">
        <div className="min-w-0 flex-1 pt-0.5">
          <RoomName
            room={room}
            title={title}
            names={names}
            editing={editing}
            setEditing={setEditing}
          />
          {second ? <p className="mt-0.5 truncate text-[12.5px] text-ash-400">{second}</p> : null}
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
            <RoomMenu room={room} onRename={() => setEditing(true)} />
          </>
        )}
        <PanelClose />
      </div>
      {room.kind === "crew" ? <CrewRooms room={room} /> : null}
    </header>
  );
}

/**
 * The header's room menu (SQOL-U3): the voice settings, a crew room's "Adı Değiştir" (its title is
 * the crew's name), the room's sharing rule (SQOL-U2's "Kim Paylaşabilir"), and, last and in flare,
 * "Odayı Kapat" with its one sentence ("Oda herkes için kapanır."). The footer keeps "Ayrıl" as its
 * only red action.
 */
function RoomMenu({ room, onRename }: { room: VoiceRoomView; onRename: () => void }) {
  const { t } = useTranslation();
  const closePanel = useVoiceUi((s) => s.closePanel);
  const navigate = useNavigate();
  const stage = useVoiceUi((s) => isOneToOne(s.state));
  const [asking, setAsking] = useState(false);
  const [busy, setBusy] = useState(false);
  // SQOL-U2: "Kim Paylaşabilir" › "Herkes" / "Lider ve Moderatörler" (a friend room's leader).
  const shareRule = useShareRuleItems();
  const items: Array<MenuItem | null> = [
    {
      label: t("voice.room.settings"),
      icon: <GearSix size={16} />,
      onSelect: () => {
        closePanel();
        void navigate({ to: "/settings", search: { section: "voice" } });
      },
    },
    ...(renameInMenu(room)
      ? [{ label: t("voice.room.rename"), icon: <PencilSimple size={16} />, onSelect: onRename }]
      : []),
    ...(shareRule.length ? [null, ...shareRule] : []),
    ...(closeInMenu(room, stage)
      ? [
          null,
          {
            label: t("voice.room.close"),
            icon: <XCircle size={16} />,
            danger: true,
            onSelect: () => setAsking(true),
          },
        ]
      : []),
  ];
  return (
    <>
      <Menu
        items={items}
        label={t("roomChat.panel.menu")}
        trigger={
          <IconButton
            label={t("roomChat.panel.menu")}
            icon={<DotsThree size={20} weight="bold" />}
          />
        }
      />
      <AskDialog
        open={asking}
        onOpenChange={(open) => {
          if (!busy) setAsking(open);
        }}
        title={t("voice.room.close")}
        description={t("voice.room.closeAsk")}
        closeLabel={t("common.close")}
        width={400}
        footer={
          <>
            <Button variant="ghost" onClick={() => setAsking(false)}>
              {t("voice.room.cancel")}
            </Button>
            <Button
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
          </>
        }
      />
    </>
  );
}

// --- the bottom -------------------------------------------------------------------------------

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

/**
 * SQOL-U3: the footer's actions as tiles, the glyph above the words, so a label that says what a
 * press does ("Mikrofonu Kapat", "Sağırlaştırmayı Kaldır") fits a 360 px panel on two lines.
 */
const footerTile =
  "h-auto min-h-[54px] w-full flex-col justify-start gap-1 whitespace-normal px-1.5 pt-2 pb-1.5";
const footerWords = "block text-center text-[12px] leading-[1.15]";

function RoomFooter({ room }: { room: VoiceRoomView }) {
  const { t } = useTranslation();
  const { me } = room;
  // OVL-NEXT: no push-to-talk key in a push-to-talk-only room keeps it closed too.
  const closed = micClosedLabel(me, t);
  const micOff = me.mute || me.deaf || closed !== null;
  // SQOL-U3: each label says what a press does, from the state it is in (as the pill's buttons
  // and Discord's do), so no `aria-pressed` beside it: a label that changes with its state and a
  // pressed state would say the state twice, one of them backwards.
  const { mic: micAction, deaf: deafAction } = footerActions(me, closed !== null, t);
  const volume = useOutputVolumeItem();
  const threshold = useVoicePrefs((s) => meterThreshold(s.settings));
  // VOICE-CALL: on the call's stage the footer is its only way out ("İptal" while it rings).
  const stage = useVoiceUi((s) => isOneToOne(s.state));
  const ringing = useVoiceUi((s) => Boolean(s.state.calling)) && stage;
  // SQOL-U3: a wide panel keeps its buttons together in the middle (not 300 px tiles).
  const { layout } = usePanelChrome();
  return (
    <footer className="shrink-0 border-t border-white/8 px-4 pt-3 pb-3.5">
      <div className={cn(layout !== "stacked" && "mx-auto w-full max-w-[600px]")}>
        {/* SHARE: the player's shares and "Ekranını paylaş". */}
        <ShareFooterRow />
        <div className="grid grid-cols-3 gap-2">
          {/* Closed by a moderator or a missing push-to-talk key: why, in its tooltip and name. */}
          <Tooltip content={closed} disabled={closed === null}>
            <Button
              size="lg"
              variant="secondary"
              aria-label={closed === null ? undefined : `${micAction}. ${closed}`}
              aria-disabled={closed !== null || undefined}
              className={cn(footerTile, closed !== null && "opacity-60")}
              icon={
                // What the microphone hears, beside its glyph (VOICE-CALL); still while it is off.
                <span className="flex items-center gap-1">
                  {micOff ? (
                    <MicrophoneSlash size={17} weight="bold" className="text-flare-400" />
                  ) : (
                    <Microphone size={17} weight="bold" />
                  )}
                  <VoiceMeter threshold={threshold} className={cn(micOff && "opacity-60")} />
                </span>
              }
              onClick={closed !== null ? undefined : () => void toggleMute()}
            >
              <span className={footerWords}>{micAction}</span>
            </Button>
          </Tooltip>
          <ContextMenu items={volume} disabled={!volume.length}>
            <Button
              size="lg"
              variant="secondary"
              className={footerTile}
              icon={
                me.deaf ? (
                  <HeadphonesSlash size={17} className="text-flare-400" />
                ) : (
                  <Headphones size={17} weight="bold" />
                )
              }
              onClick={() => void toggleDeaf()}
            >
              <span className={footerWords}>{deafAction}</span>
            </Button>
          </ContextMenu>
          <Button
            size="lg"
            variant="secondary"
            className={footerTile}
            // Leaving ends nothing for the others: a plate with a red glyph, the footer's only red
            // action ("Odayı Kapat" is the header menu's last row).
            icon={<PhoneDisconnect size={17} weight="bold" className="text-flare-400" />}
            aria-label={ringing ? t("voice.call.cancelLabel") : undefined}
            onClick={() => void leaveRoom()}
          >
            <span className={footerWords}>
              {ringing ? t("voice.room.cancelRing") : t("voice.room.leave")}
            </span>
          </Button>
        </div>
      </div>
    </footer>
  );
}

// --- the pages --------------------------------------------------------------------------------

function RoomPage({ room }: { room: VoiceRoomView }) {
  const { t } = useTranslation();
  const calling = useVoiceUi((s) => s.state.calling);
  const faces = useFaces([...room.people.map((p) => p.id), ...(calling ? [calling.friend] : [])]);
  const nameOf = (id: string) => faces.find((f) => f.id === id)?.name ?? t("voice.someone");
  // SQOL-U3: a crew room's title is the crew's name; its chips say which room.
  const crewName = useSocialUi((s) => s.view.crews.find((c) => c.id === room.crew)?.name);
  const title = panelTitle(room, crewName, nameOf, t, calling?.friend);
  // SQOL-U3: a chat under the people when the room has one (or the call's friend's messages).
  const chat = usePanelChat(room);
  // SQOL-U3: a wide panel (600 px and more) puts the people beside the chat; crossing the width
  // only reflows: the chat stays as it was (its scroll, its field), nothing animates.
  const { layout } = usePanelChrome();
  const columns = layout !== "stacked";
  return (
    <>
      <RoomHeader
        room={room}
        title={title}
        names={titleNames(room, nameOf)}
        counted={chat.kind !== "none"}
      />
      <AnimatePresence initial={false} mode="wait">
        {chat.kind === "none" ? (
          <motion.div
            key="people"
            className="flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain px-3 pt-3 pb-4"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.12) }}
          >
            <Notices room={room} />
            <People room={room} />
          </motion.div>
        ) : (
          <motion.div
            key="chat"
            className={cn("flex min-h-0 flex-1", columns ? "flex-row" : "flex-col")}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: enter(0.2) }}
            exit={{ opacity: 0, transition: leave(0.12) }}
          >
            {columns ? (
              <PeopleColumn key="people-column" room={room} wider={layout === "wider"} />
            ) : (
              <Fragment key="people-stack">
                <div className="shrink-0 px-3 pt-3 empty:hidden">
                  <Notices room={room} />
                </div>
                <PeopleSection room={room} />
              </Fragment>
            )}
            <div key="chat-column" className="flex min-h-0 min-w-0 flex-1 flex-col">
              <PanelChatSection room={room} chat={chat} centered={layout === "wider"} />
            </div>
          </motion.div>
        )}
      </AnimatePresence>
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
        <PanelTitle className="min-w-0 flex-1 truncate font-display text-[19px] leading-tight font-semibold text-bone-50">
          {t("voice.invite.title")}
        </PanelTitle>
        <PanelClose />
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

/** The room and "Davet et" over it, sliding as the friends panel's pages do. */
function PanelPages({ room }: { room: VoiceRoomView }) {
  const page = useVoiceUi((s) => s.page);
  const direction = useVoiceUi((s) => s.direction);
  const { reduce } = useMotionPrefs();
  const variants = pageVariants(reduce);
  return (
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
          {page === "invite" ? <InvitePage room={room} /> : <RoomPage room={room} />}
        </motion.div>
      </AnimatePresence>
    </ReducedMotionConfig>
  );
}

// --- the panel's two shells ---------------------------------------------------------------------

/** The room and "Davet et": the same while only the panel's width changes (a drag's frames). */
const PanelPagesKept = memo(PanelPages);

/** How wide the panel is now, and how wide it may be (panel-size.ts). */
interface Sizing {
  place: PanelPlace;
  width: number;
  limits: WidthLimits;
  windowWidth: number;
}

/** The panel's chrome for its pages: a new value only when one of them changes. */
function useChrome(docked: boolean, shown: boolean, width: number): PanelChrome {
  const close = useVoiceUi((s) => s.closePanel);
  const layout = layoutFor(width);
  return useMemo(() => ({ docked, shown, close, layout }), [docked, shown, close, layout]);
}

/** The panel as a sheet over the page (no stage on the page): a modal dialog, as before. */
function SheetShell({
  room,
  open,
  arrival,
  sizing,
}: {
  room: VoiceRoomView | null;
  open: boolean;
  arrival: Arrival;
  sizing: Sizing;
}) {
  const close = useVoiceUi((s) => s.closePanel);
  const returnFocus = useVoiceUi((s) => s.returnFocus);
  const cueChatFocus = useVoiceUi((s) => s.cueChatFocus);
  const animate = useInteractionMotion();
  const motionOf = sheetMotion(!animate);
  const chrome = useChrome(false, open, sizing.width);
  return (
    <Dialog.Root open={open} onOpenChange={(next) => !next && close()}>
      <AnimatePresence>
        {open && room ? (
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
                  // The stage went while the chat's field had the focus: the field keeps it.
                  if (arrival.focus === "field") cueChatFocus();
                  else focusPanel(event.currentTarget as HTMLElement);
                }}
                onCloseAutoFocus={(event) => {
                  // Still open: its shell changed in place (the stage came) or the full-screen
                  // stage hides it. The focus stays where it went.
                  if (useVoiceUi.getState().panel) {
                    event.preventDefault();
                    return;
                  }
                  // Another layer took over meanwhile ("Ekip Sohbeti" opened the friends panel).
                  const now = document.activeElement;
                  if (now instanceof Element && now.closest("[role='dialog']")) {
                    event.preventDefault();
                    return;
                  }
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
                  data-chat-host=""
                  data-room-panel=""
                  style={{ width: sizing.width }}
                  className="glass-strong fixed top-0 right-0 bottom-0 z-40 flex max-w-[94vw] flex-col overflow-hidden shadow-[-30px_0_80px_rgb(34_6_6/0.5)] outline-none"
                  {...motionOf}
                  // Its shell changed in place: it is already there, nothing slides in.
                  {...(arrival.moved ? { initial: false } : {})}
                >
                  <PanelResizer {...sizing} />
                  <PanelChromeProvider value={chrome}>
                    <PanelPagesKept room={room} />
                  </PanelChromeProvider>
                </GatedMotion.aside>
              </Dialog.Content>
            </Dialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </Dialog.Root>
  );
}

/**
 * The panel docked beside the stage (SQOL §1.3 "Yanaşık panel"): no scrim, no focus trap, a press
 * outside leaves it; "Ses Odası" as a complementary landmark after the stage in the page's order;
 * X and Esc (with the focus inside) close it, the focus going back where it came from.
 */
function DockShell({
  room,
  open,
  arrival,
  sizing,
}: {
  room: VoiceRoomView | null;
  open: boolean;
  arrival: Arrival;
  sizing: Sizing;
}) {
  const { t } = useTranslation();
  const close = useVoiceUi((s) => s.closePanel);
  const returnFocus = useVoiceUi((s) => s.returnFocus);
  const cueChatFocus = useVoiceUi((s) => s.cueChatFocus);
  const animate = useInteractionMotion();
  const panel = useRef<HTMLElement>(null);
  const motionOf = sheetMotion(!animate);
  const chrome = useChrome(true, open, sizing.width);

  // Opened: the focus goes in (the panel, or the chat's field when it was asked for or had it).
  // biome-ignore lint/correctness/useExhaustiveDependencies: on opening only
  useLayoutEffect(() => {
    if (!open) return;
    const element = panel.current;
    if (arrival.focus === "field") cueChatFocus();
    else if (element && (!arrival.moved || arrival.focus === "panel")) focusPanel(element);
  }, [open]);

  // Closed with the focus inside: it goes back to where the panel was opened from.
  const wasOpen = useRef(open);
  useEffect(() => {
    if (wasOpen.current && !open) {
      const element = panel.current;
      if (element?.contains(document.activeElement) && returnFocus?.isConnected) {
        returnFocus.focus();
      }
    }
    wasOpen.current = open;
  }, [open, returnFocus]);

  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    // A layer inside (a menu, the emoji picker, a field's own Esc) took it already.
    if (event.key !== "Escape" || event.defaultPrevented) return;
    event.preventDefault();
    close();
  };

  return (
    <>
      {/* The sheet's scrim leaves as the panel docks (its shell changed in place). */}
      {arrival.moved && !arrival.back && open ? (
        <motion.div
          aria-hidden
          className="pointer-events-none fixed inset-0 z-[22] bg-obsidian-950/55"
          initial={{ opacity: 1 }}
          animate={{ opacity: 0, transition: { duration: 0.18 } }}
        />
      ) : null}
      <AnimatePresence>
        {open && room ? (
          <EntranceGate key="dock">
            <GatedMotion.aside
              ref={panel}
              tabIndex={-1}
              role="complementary"
              aria-label={t("roomChat.panel.label")}
              data-chat-host=""
              data-room-panel=""
              onKeyDown={onKeyDown}
              style={{ width: sizing.width }}
              className="glass-strong fixed top-titlebar right-0 bottom-0 z-[25] flex flex-col overflow-hidden border-l border-white/8 shadow-[-24px_0_60px_rgb(34_6_6/0.38)] outline-none"
              {...motionOf}
              {...(arrival.moved ? { initial: false } : {})}
            >
              <PanelResizer {...sizing} />
              <PanelChromeProvider value={chrome}>
                <PanelPagesKept room={room} />
              </PanelChromeProvider>
            </GatedMotion.aside>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </>
  );
}

export function RoomSheet() {
  const panel = useVoiceUi((s) => s.panel);
  const room = useVoiceUi((s) => s.state.room);
  const setDock = useVoiceUi((s) => s.setDock);
  const stage = useStageMode();
  const docked = stage === "page";
  // A stage over the whole window covers everything: the panel waits behind it, still open.
  const open = panel && room !== null && stage !== "full";
  const behind = panel && room !== null && stage === "full";
  // The sheet keeps the last room while it slides away.
  const last = useRef<VoiceRoomView | null>(null);
  if (room) last.current = room;
  const shown = room ?? last.current;

  // SQOL-U3: the width the player chose for this place (or today's), within what the window
  // leaves it; while the separator is dragged, the drag's.
  const place: PanelPlace = docked ? "docked" : "modal";
  const windowWidth = useWindowWidth();
  const limits = widthLimits(place, windowWidth);
  const chosen = usePanelSize((s) => s.chosen[place]);
  const dragging = usePanelSize((s) => s.dragging);
  const width = clampWidth(dragging ?? chosen, limits);
  const sizing: Sizing = { place, width, limits, windowWidth };

  // The stage leaves the docked panel's width free: from the start of its slide in, until the
  // start of its slide out, following a drag frame by frame. Toasts keep clear of the width the
  // player settled on.
  const dock = docked && open ? width : 0;
  useEffect(() => setDock(dock), [setDock, dock]);
  useEffect(() => () => useVoiceUi.getState().setDock(0), []);
  useToastsBeside(docked && open ? clampWidth(chosen, limits) : 0);

  // The stage came or went while the panel showed: its shell changes in place. Where the focus was
  // in it is read now, before the old shell goes.
  const placed = useRef({ docked, open, behind });
  const arrival = useRef<Arrival>(FRESH);
  arrival.current =
    arrivalOf(placed.current, { docked, open }, () => {
      const active = typeof document === "undefined" ? null : document.activeElement;
      const inside = active?.closest("[data-room-panel]") ? active : null;
      return inside ? (inside instanceof HTMLTextAreaElement ? "field" : "panel") : null;
    }) ?? arrival.current;
  useEffect(() => {
    placed.current = { docked, open, behind };
  });

  return docked ? (
    <DockShell room={shown} open={open} arrival={arrival.current} sizing={sizing} />
  ) : (
    <SheetShell room={shown} open={open} arrival={arrival.current} sizing={sizing} />
  );
}
