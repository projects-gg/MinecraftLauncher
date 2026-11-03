// VOICE-ROOMS: a crew room's menu (the crew page's row and its "⋯"; the room panel's tab with its
// own action and the order only): the room's own action first ("Katıl" / "Ayrıl"), then by role
// "Odayı düzenle", "Yukarı taşı" / "Aşağı taşı", and "Odayı sil" last (voice.md §4; "Genel" has only
// its own action). Editing and the one sentence before deleting a room with people in it happen in
// the row itself, as the crew page's plan form and the room panel's rename do (no dialog: neither
// needs to interrupt anything); an empty room goes at once with "Geri al".
import {
  ArrowDown,
  ArrowLeft,
  ArrowRight,
  ArrowUp,
  Headset,
  PencilSimple,
  SignOut,
  Trash,
} from "@phosphor-icons/react";
import { Button, Input, type MenuItem, Select } from "@pl/ui";
import { type FormEvent, type RefObject, useEffect, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { VoiceCrewRoom } from "@/lib/api/voice-types";
import { editCrewRoom, joinCrewRoom, leaveRoom, orderCrewRooms, removeCrewRoom } from "./actions";
import { crewRoomName, limitChoices, moveRoom, type RoomPowers, roomNameFits } from "./rules";

/** Drops separators at the ends and doubled ones. */
function tidy(items: Array<MenuItem | null>): Array<MenuItem | null> {
  const out: Array<MenuItem | null> = [];
  for (const item of items) {
    if (item === null && (out.length === 0 || out[out.length - 1] === null)) continue;
    out.push(item);
  }
  while (out.length && out[out.length - 1] === null) out.pop();
  return out;
}

/** What a room's menu acts on and may do. */
export interface RoomMenuInput {
  crew: string;
  room: VoiceCrewRoom;
  /** The player is in it ("Ayrıl" instead of "Katıl"). */
  here: boolean;
  powers: RoomPowers;
  /** The crew's rooms ("Genel" aside) in the order shown, for "Yukarı taşı" / "Aşağı taşı". */
  order: readonly string[];
  /** The rooms stand side by side (the room panel's tabs): "Sola taşı" / "Sağa taşı", no keys. */
  across?: boolean;
  onEdit: () => void;
  onRemove: () => void;
}

/** Alt+↑ / Alt+↓ as the menu writes them (`MenuItem.shortcut`). */
export const MOVE_UP_KEYS = "Alt ↑";
export const MOVE_DOWN_KEYS = "Alt ↓";

export function roomMenuItems(
  input: RoomMenuInput,
  t: (key: string) => string,
): Array<MenuItem | null> {
  const { crew, room, here, powers, order } = input;
  const across = input.across === true;
  const at = order.indexOf(room.id);
  const move = (step: -1 | 1) => {
    const next = moveRoom(order, room.id, step);
    if (next) void orderCrewRooms(crew, next);
  };
  return tidy([
    here
      ? {
          label: t("voice.crew.leave"),
          icon: <SignOut size={16} />,
          onSelect: () => void leaveRoom(),
        }
      : {
          label: t("voice.crew.join"),
          icon: <Headset size={16} />,
          onSelect: () => void joinCrewRoom(crew, room.general ? null : room.id),
        },
    null,
    powers.edit
      ? { label: t("voice.rooms.edit"), icon: <PencilSimple size={16} />, onSelect: input.onEdit }
      : null,
    powers.order
      ? {
          label: t(across ? "voice.rooms.moveLeft" : "voice.rooms.moveUp"),
          icon: across ? <ArrowLeft size={16} /> : <ArrowUp size={16} />,
          ...(across ? {} : { shortcut: MOVE_UP_KEYS }),
          disabled: at <= 0,
          onSelect: () => move(-1),
        }
      : null,
    powers.order
      ? {
          label: t(across ? "voice.rooms.moveRight" : "voice.rooms.moveDown"),
          icon: across ? <ArrowRight size={16} /> : <ArrowDown size={16} />,
          ...(across ? {} : { shortcut: MOVE_DOWN_KEYS }),
          disabled: at < 0 || at >= order.length - 1,
          onSelect: () => move(1),
        }
      : null,
    null,
    powers.remove
      ? {
          label: t("voice.rooms.remove"),
          icon: <Trash size={16} />,
          danger: true,
          onSelect: input.onRemove,
        }
      : null,
  ]);
}

/** Whether the menu offers more than the room's own action (its "⋯" shows then). */
export function hasRoomPowers(powers: RoomPowers): boolean {
  return powers.edit || powers.order || powers.remove;
}

/**
 * Esc leaves the row's form, not the page or panel around it (as the room panel's rename). A list
 * opened from the form (its "4 kişi") lies outside it: its Esc closes only the list.
 */
function useEscape(within: RefObject<HTMLElement | null>, onEscape: () => void) {
  const latest = useRef(onEscape);
  latest.current = onEscape;
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      const target = event.target;
      const loose = target === document.body || target === document.documentElement;
      if (!loose && !(target instanceof Node && within.current?.contains(target))) return;
      event.preventDefault();
      event.stopPropagation();
      latest.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [within]);
}

/**
 * "Odayı düzenle" in the row: the room's name (opened with the name as shown: an unchanged one is
 * not sent, so an English "Room 2" never renames the crew's "Oda 2") and its people at most, from
 * 2 to the crew room maximum (the maximum itself: no limit of its own).
 */
export function RoomEditForm({
  crew,
  crewMax,
  room,
  onDone,
}: {
  crew: string;
  crewMax: number;
  room: VoiceCrewRoom;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const ids = useId();
  const shownName = crewRoomName(room, t);
  const [name, setName] = useState(shownName);
  const [limit, setLimit] = useState(String(Math.min(room.max ?? crewMax, crewMax)));
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const box = useRef<HTMLFormElement>(null);
  useEscape(box, onDone);
  const choices = limitChoices(crewMax)
    .filter((value): value is number => value !== null)
    .map((value) => ({
      value: String(value),
      label: t("voice.rooms.limitValue", { count: value }),
    }));

  const save = async (event: FormEvent) => {
    event.preventDefault();
    if (busy) return;
    const nameChanged = name.trim() !== shownName;
    const max = Number(limit) >= crewMax ? null : Number(limit);
    if (!nameChanged && max === room.max) {
      onDone();
      return;
    }
    if (nameChanged && !roomNameFits(name)) {
      setError(t("voice.refused.invalidName"));
      return;
    }
    setBusy(true);
    const refused = await editCrewRoom(crew, room.id, nameChanged ? name : room.name, max);
    setBusy(false);
    if (refused) setError(refused);
    else onDone();
  };

  return (
    <form ref={box} onSubmit={(event) => void save(event)} className="flex flex-col gap-2 py-1">
      <div className="flex items-center gap-2">
        <Input
          id={`${ids}-name`}
          autoFocus
          value={name}
          maxLength={24}
          invalid={Boolean(error)}
          aria-label={t("voice.rooms.name")}
          aria-describedby={error ? `${ids}-error` : undefined}
          placeholder={t("voice.room.namePlaceholder")}
          onChange={(event) => {
            setName(event.target.value);
            setError(null);
          }}
          className="h-8 min-w-0 flex-1"
        />
        <Select
          value={limit}
          onValueChange={setLimit}
          options={choices}
          label={t("voice.rooms.limit")}
          className="h-8 w-[104px] shrink-0"
        />
      </div>
      <div className="flex items-center gap-2">
        <p
          id={`${ids}-error`}
          role={error ? "alert" : undefined}
          className="min-w-0 flex-1 text-[12px] leading-snug text-flare-300"
        >
          {error}
        </p>
        <Button size="sm" variant="ghost" type="button" onClick={onDone}>
          {t("voice.rooms.cancel")}
        </Button>
        <Button size="sm" variant="primary" type="submit" disabled={busy || !name.trim()}>
          {t("voice.rooms.save")}
        </Button>
      </div>
    </form>
  );
}

/**
 * "Odayı sil" of a room with people in it, in the row: one sentence, they go to "Genel". `onDone`
 * says whether the room went.
 */
export function RoomRemoveAsk({
  crew,
  room,
  onDone,
}: {
  crew: string;
  room: VoiceCrewRoom;
  onDone: (removed: boolean) => void;
}) {
  const { t } = useTranslation();
  const ids = useId();
  const [busy, setBusy] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  useEscape(box, () => onDone(false));
  return (
    <div ref={box} className="flex min-h-8 items-center gap-2">
      <p id={`${ids}-ask`} className="min-w-0 flex-1 text-[13px] leading-snug text-bone-100">
        {t("voice.rooms.removeAsk")}
      </p>
      {/* The safe answer takes the focus: Enter never deletes by accident. */}
      <Button size="sm" variant="ghost" autoFocus onClick={() => onDone(false)}>
        {t("voice.rooms.cancel")}
      </Button>
      <Button
        size="sm"
        variant="danger"
        disabled={busy}
        aria-describedby={`${ids}-ask`}
        onClick={async () => {
          setBusy(true);
          const removed = await removeCrewRoom(crew, room.id);
          setBusy(false);
          onDone(removed);
        }}
      >
        {t("voice.rooms.remove")}
      </Button>
    </div>
  );
}
