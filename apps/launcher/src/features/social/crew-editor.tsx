// "Ekip kur" and the crew's "Düzenle": a live preview on top (the mark, the name, the tag, the
// motto), then the name and the tag, the motto with its counter, and "Görünüm": 12 colours in one
// row, 4 frames, 36 emblems (9 × 4), with "Rastgele" beside the heading. Pickers are APG radio
// groups (arrows, Home/End; Tab leaves); the chosen tile wears a bone ring that fades in, a press
// squeezes to 94 %. The preview's mark cross-fades in place when the look changes (one fade for
// "Rastgele"). A service without looks gets today's editor: name, tag and the first 20 emblems.
// The crew colour shows on the preview alone: the frame and emblem tiles are shapes in bone (chosen)
// and ash, so an amber crew never reads as a second gold beside "Ekip kur" (TASTE-SWEEP SMALLS-L r2).
import { DiceFive } from "@phosphor-icons/react";
import { Button, cn, Tooltip } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import {
  type FormEvent,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
  useId,
  useRef,
} from "react";
import { useTranslation } from "react-i18next";
import type { CrewLook } from "@/lib/api/social-types";
import {
  COLORS,
  CREW_COLORS,
  type CrewColor,
  type CrewFrame,
  crewColor,
  crewFrame,
  type Emblem,
  edgeWidth,
  emblemsFor,
  FRAME_SHAPES,
  FRAMES,
  isEmblem,
  shuffleLook,
} from "./crew-look";
import { rise, swap, useSocialMotion } from "./motion";
import { CrewMark, emblemIcon, TeamTag } from "./team-emblem";
import {
  formatTeamTag,
  MOTTO_MAX,
  mottoLength,
  mottoProblem,
  TEAM_NAME_MAX,
  TEAM_TAG_MAX,
  teamNameProblem,
  teamTagValid,
} from "./teams";

export const fieldClass =
  "h-10 w-full min-w-0 rounded-md border border-white/10 bg-obsidian-800/80 px-3 text-[14px] text-bone-50 transition-[border-color,box-shadow] duration-150 outline-none placeholder:text-ash-500 hover:border-white/16 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-300/25 disabled:opacity-55 aria-[invalid=true]:border-flare-400/70";

const tileClass =
  "relative flex cursor-pointer items-center justify-center outline-none transition-[background-color,box-shadow,transform,scale] duration-150 focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.94]";

/**
 * The ring of a chosen tile: bone, fading in and out (opacity only). A choice is never gold: the
 * screen's one gold is its main action (design.md §2; the keyboard's focus ring stays gold).
 */
function ChosenRing({ on, className }: { on: boolean; className?: string }) {
  return (
    <span
      aria-hidden
      className={cn(
        "pointer-events-none absolute -inset-[3px] ring-2 ring-bone-100 transition-opacity duration-150",
        on ? "opacity-100" : "opacity-0",
        className,
      )}
    />
  );
}

/** Arrow keys, Home and End inside a radio group of tiles (`columns` per row). */
function useRadioKeys<T extends string>(
  items: readonly T[],
  columns: number,
  onChange: (value: T) => void,
  group: RefObject<HTMLDivElement | null>,
) {
  return (event: KeyboardEvent<HTMLButtonElement>, index: number) => {
    const step: Record<string, number> = {
      ArrowRight: 1,
      ArrowLeft: -1,
      ArrowDown: columns,
      ArrowUp: -columns,
    };
    let next: number;
    if (event.key === "Home") next = 0;
    else if (event.key === "End") next = items.length - 1;
    else if (step[event.key] !== undefined) {
      next = (index + (step[event.key] ?? 0) + items.length) % items.length;
    } else return;
    event.preventDefault();
    const value = items[next] as T;
    onChange(value);
    group.current?.querySelector<HTMLElement>(`[data-value="${value}"]`)?.focus();
  };
}

function Label({ children, aside }: { children: ReactNode; aside?: ReactNode }) {
  return (
    <p className="mb-1.5 flex items-baseline justify-between gap-2 text-[12.5px] text-ash-400">
      <span>{children}</span>
      {aside}
    </p>
  );
}

function ColorPicker({ value, onChange }: { value: CrewColor; onChange: (c: CrewColor) => void }) {
  const { t } = useTranslation();
  const group = useRef<HTMLDivElement>(null);
  const onKey = useRadioKeys(COLORS, COLORS.length, onChange, group);
  return (
    <div
      ref={group}
      role="radiogroup"
      aria-label={t("teams.look.color")}
      className="flex flex-wrap gap-[5px]"
    >
      {COLORS.map((color, index) => {
        const chosen = color === value;
        const swatch = CREW_COLORS[color];
        return (
          <Tooltip key={color} content={t(`teams.colors.${color}`)}>
            {/* biome-ignore lint/a11y/useSemanticElements: a colour tile; the group handles arrows (APG radio group). */}
            <button
              type="button"
              role="radio"
              aria-checked={chosen}
              aria-label={t(`teams.colors.${color}`)}
              data-value={color}
              tabIndex={chosen ? 0 : -1}
              onClick={() => onChange(color)}
              onKeyDown={(event) => onKey(event, index)}
              className={cn(tileClass, "size-[25px] rounded-[6px]")}
              style={{ background: swatch.fill, boxShadow: `inset 0 0 0 1.5px ${swatch.edge}` }}
            >
              <ChosenRing on={chosen} className="rounded-[8px]" />
            </button>
          </Tooltip>
        );
      })}
    </div>
  );
}

/** A frame's shape alone, in its tile's text colour (bone when chosen, ash otherwise). */
function FrameShape({ frame, chosen }: { frame: CrewFrame; chosen: boolean }) {
  const size = 30;
  return (
    <svg
      aria-hidden="true"
      viewBox="0 0 40 40"
      width={size}
      height={size}
      className={cn(
        "shrink-0 transition-colors duration-150",
        chosen ? "text-bone-50" : "text-ash-300",
      )}
    >
      <path
        d={FRAME_SHAPES[frame].path}
        fill="currentColor"
        fillOpacity={0.1}
        stroke="currentColor"
        strokeWidth={edgeWidth(size)}
        strokeLinejoin="round"
      />
    </svg>
  );
}

function FramePicker({ value, onChange }: { value: CrewFrame; onChange: (f: CrewFrame) => void }) {
  const { t } = useTranslation();
  const group = useRef<HTMLDivElement>(null);
  const onKey = useRadioKeys(FRAMES, FRAMES.length, onChange, group);
  return (
    <div
      ref={group}
      role="radiogroup"
      aria-label={t("teams.look.frame")}
      className="grid grid-cols-4 gap-2"
    >
      {FRAMES.map((frame, index) => {
        const chosen = frame === value;
        return (
          // biome-ignore lint/a11y/useSemanticElements: a frame tile; the group handles arrows (APG radio group).
          <button
            key={frame}
            type="button"
            role="radio"
            aria-checked={chosen}
            data-value={frame}
            tabIndex={chosen ? 0 : -1}
            onClick={() => onChange(frame)}
            onKeyDown={(event) => onKey(event, index)}
            className={cn(
              tileClass,
              "flex-col gap-1 rounded-md py-2",
              chosen ? "bg-white/[0.06]" : "bg-white/[0.03] hover:bg-white/[0.06]",
            )}
          >
            <ChosenRing on={chosen} className="rounded-[9px]" />
            <FrameShape frame={frame} chosen={chosen} />
            <span
              className={cn(
                "text-[12px] transition-colors duration-150",
                chosen ? "text-bone-50" : "text-ash-300",
              )}
            >
              {t(`teams.frames.${frame}`)}
            </span>
          </button>
        );
      })}
    </div>
  );
}

/** The emblems as shapes: the chosen one bone on the quiet plate, the others ash on obsidian. */
function EmblemPicker({
  value,
  emblems,
  onChange,
}: {
  value: Emblem;
  emblems: readonly Emblem[];
  onChange: (emblem: Emblem) => void;
}) {
  const { t } = useTranslation();
  const group = useRef<HTMLDivElement>(null);
  const columns = 9;
  const onKey = useRadioKeys(emblems, columns, onChange, group);
  return (
    <div
      ref={group}
      role="radiogroup"
      aria-label={t("teams.create.emblem")}
      className="grid grid-cols-9 gap-1.5"
    >
      {emblems.map((emblem, index) => {
        const chosen = emblem === value;
        const Glyph = emblemIcon(emblem);
        return (
          // biome-ignore lint/a11y/useSemanticElements: an icon tile; the group handles arrows (APG radio group).
          <button
            key={emblem}
            type="button"
            role="radio"
            aria-checked={chosen}
            aria-label={t(`teams.emblems.${emblem}`)}
            data-value={emblem}
            tabIndex={chosen ? 0 : -1}
            onClick={() => onChange(emblem)}
            onKeyDown={(event) => onKey(event, index)}
            className={cn(
              tileClass,
              "aspect-square rounded-md",
              chosen
                ? "bg-white/[0.06]"
                : "bg-obsidian-950/70 ring-1 ring-white/7 hover:bg-obsidian-800",
            )}
          >
            <ChosenRing on={chosen} className="rounded-[9px]" />
            <Glyph
              size={19}
              weight="fill"
              className={cn(
                "transition-colors duration-150",
                chosen ? "text-bone-50" : "text-ash-300",
              )}
            />
          </button>
        );
      })}
    </div>
  );
}

/** The card a crew gets: mark, name, tag, motto. Live while it is being made or edited. */
export function CrewPreview({
  name,
  tag,
  look,
  motto,
  meta,
  placeholder = false,
  size = 60,
}: {
  name: string;
  tag: string;
  look: CrewLook;
  motto?: string;
  meta: string;
  /** No name typed yet: the name's place says what goes there, quietly. */
  placeholder?: boolean;
  size?: number;
}) {
  const { reduce } = useSocialMotion();
  return (
    <div className="flex items-center gap-3.5">
      <span className="relative flex" style={{ width: size, height: size }}>
        <AnimatePresence mode="popLayout" initial={false}>
          <motion.span
            key={`${look.emblem}:${look.color}:${look.frame}`}
            className="absolute inset-0 flex"
            {...swap(reduce, 0.2)}
          >
            <CrewMark look={look} size={size} />
          </motion.span>
        </AnimatePresence>
      </span>
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
          {tag ? <TeamTag tag={tag} color={look.color} /> : null}
        </p>
        {motto ? (
          <p className="mt-0.5 truncate text-[13px] text-ash-300" data-selectable>
            {motto}
          </p>
        ) : null}
        <p className="mt-0.5 text-[12.5px] text-ash-400">{meta}</p>
      </div>
    </div>
  );
}

export interface CrewDraft {
  name: string;
  tag: string;
  motto: string;
  emblem: Emblem;
  color: CrewColor;
  frame: CrewFrame;
}

/** What a crew's editor starts from. */
export function draftOf(team: {
  name: string;
  tag: string;
  motto?: string;
  emblem: string;
  color?: string;
  frame?: string;
}): CrewDraft {
  return {
    name: team.name,
    tag: team.tag,
    motto: team.motto ?? "",
    emblem: isEmblem(team.emblem) ? team.emblem : "shield",
    color: crewColor(team.color),
    frame: crewFrame(team.frame),
  };
}

export const NEW_CREW: CrewDraft = {
  name: "",
  tag: "",
  motto: "",
  emblem: "shield",
  color: "gold",
  frame: "cut",
};

/**
 * The editor's fields under the preview. `look`: the service offers looks (colours, frames,
 * mottos, 36 emblems). `texts`: the name and the tag may change (the leader; a moderator edits only
 * the look and the motto).
 */
export function CrewEditor({
  draft,
  onChange,
  look,
  texts,
  meta,
  showErrors,
  footer,
  onSubmit,
}: {
  draft: CrewDraft;
  onChange: (draft: CrewDraft) => void;
  look: boolean;
  texts: boolean;
  /** Under the preview's name ("1 üye", "12/30 üye"). */
  meta: string;
  /** Problems show once the player tried to send (or right away while editing). */
  showErrors: boolean;
  footer: ReactNode;
  onSubmit: () => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const nameId = useId();
  const tagId = useId();
  const mottoId = useId();
  const nameProblem = texts ? teamNameProblem(draft.name) : null;
  const tagOk = !texts || teamTagValid(draft.tag);
  const motto = look ? mottoProblem(draft.motto) : null;
  const textError = showErrors
    ? nameProblem
      ? t(`teams.create.${nameProblem}`)
      : tagOk
        ? null
        : t("teams.create.tagInvalid")
    : null;
  const mottoError = motto
    ? t(
        motto === "long"
          ? "teams.look.mottoLong"
          : motto === "chars"
            ? "teams.look.mottoChars"
            : "teams.look.mottoLetters",
      )
    : null;
  const count = mottoLength(draft.motto);
  const emblems = emblemsFor(look);
  const set = (change: Partial<CrewDraft>) => onChange({ ...draft, ...change });

  const submit = (event: FormEvent) => {
    event.preventDefault();
    onSubmit();
  };

  return (
    <form className="space-y-3.5" onSubmit={submit} noValidate>
      <CrewPreview
        name={draft.name.trim() || t("teams.create.name")}
        placeholder={!draft.name.trim()}
        tag={draft.tag}
        look={draft}
        motto={look ? draft.motto.trim() : undefined}
        meta={meta}
      />
      {texts ? (
        <div className="flex gap-2">
          <div className="min-w-0 flex-1">
            <label htmlFor={nameId} className="mb-1 block text-[12.5px] text-ash-400">
              {t("teams.create.name")}
            </label>
            <input
              id={nameId}
              value={draft.name}
              maxLength={TEAM_NAME_MAX + 8}
              onChange={(event) => set({ name: event.target.value })}
              aria-invalid={Boolean(showErrors && nameProblem)}
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
              value={draft.tag}
              maxLength={TEAM_TAG_MAX}
              onChange={(event) => set({ tag: formatTeamTag(event.target.value) })}
              aria-invalid={showErrors && !tagOk}
              autoComplete="off"
              spellCheck={false}
              className={cn(fieldClass, "font-display tracking-[0.08em] uppercase")}
            />
          </div>
        </div>
      ) : null}
      <AnimatePresence initial={false}>
        {textError ? (
          <motion.p
            key="error"
            role="alert"
            className="text-[12.5px] text-flare-300"
            {...rise(reduce, 4, 0.2)}
          >
            {textError}
          </motion.p>
        ) : null}
      </AnimatePresence>
      {look ? (
        <div>
          <label
            htmlFor={mottoId}
            className="mb-1 flex items-baseline justify-between gap-2 text-[12.5px] text-ash-400"
          >
            <span>{t("teams.look.motto")}</span>
            <span className="text-[12px] text-ash-400">{t("teams.look.mottoOptional")}</span>
          </label>
          <div className="relative">
            <input
              id={mottoId}
              value={draft.motto}
              maxLength={MOTTO_MAX + 20}
              onChange={(event) => set({ motto: event.target.value })}
              aria-invalid={Boolean(mottoError)}
              aria-describedby={`${mottoId}-count`}
              autoComplete="off"
              spellCheck={false}
              className={cn(fieldClass, "pr-14")}
            />
            <span
              id={`${mottoId}-count`}
              className={cn(
                "pointer-events-none absolute top-1/2 right-3 -translate-y-1/2 font-display text-[12px] tabular-nums transition-colors duration-150",
                count > MOTTO_MAX ? "text-flare-300" : "text-ash-400",
              )}
            >
              {t("teams.look.count", { count, max: MOTTO_MAX })}
            </span>
          </div>
          <AnimatePresence initial={false}>
            {mottoError ? (
              <motion.p
                key="motto"
                role="alert"
                className="mt-1.5 text-[12.5px] text-flare-300"
                {...rise(reduce, 4, 0.2)}
              >
                {mottoError}
              </motion.p>
            ) : null}
          </AnimatePresence>
        </div>
      ) : null}
      {look ? (
        <div className="space-y-3 pt-1">
          <div className="flex items-center justify-between">
            <h4 className="font-display text-[15px] font-semibold text-bone-50">
              {t("teams.look.title")}
            </h4>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              icon={<DiceFive size={15} />}
              onClick={() => set(shuffleLook(draft, emblems))}
            >
              {t("teams.look.shuffle")}
            </Button>
          </div>
          <div>
            <Label>{t("teams.look.color")}</Label>
            <ColorPicker value={draft.color} onChange={(color) => set({ color })} />
          </div>
          <div>
            <Label>{t("teams.look.frame")}</Label>
            <FramePicker value={draft.frame} onChange={(frame) => set({ frame })} />
          </div>
          <div>
            <Label>{t("teams.create.emblem")}</Label>
            <EmblemPicker
              value={draft.emblem}
              emblems={emblems}
              onChange={(emblem) => set({ emblem })}
            />
          </div>
        </div>
      ) : (
        <div>
          <Label>{t("teams.create.emblem")}</Label>
          <EmblemPicker
            value={emblems.includes(draft.emblem) ? draft.emblem : "shield"}
            emblems={emblems}
            onChange={(emblem) => set({ emblem })}
          />
        </div>
      )}
      {footer}
    </form>
  );
}

/** Whether a draft can be sent (the service still checks its word lists). */
export function draftReady(draft: CrewDraft, look: boolean, texts: boolean): boolean {
  if (texts && (teamNameProblem(draft.name) || !teamTagValid(draft.tag))) return false;
  return !(look && mottoProblem(draft.motto));
}
