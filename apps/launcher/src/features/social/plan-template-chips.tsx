// CREW-PLUS: the plan form's template chips (plan-templates.ts). The three built-in ones and the
// player's own (at most five), each a chamfered chip as the day chips under them; pressing one puts
// its day, time and note in the form (a chip whose fill the form holds sits on the quiet plate, as
// a chosen day does), resting on it says what it fills. "Şablonlara Ekle" keeps the form as one of
// the player's own; their chips carry the launcher's menu (right click, long press, Shift+F10):
// "Yeniden Adlandır" in place, "Sil" with "Geri Al" in its toast.
import { Check, PencilSimple, Plus, Trash, X } from "@phosphor-icons/react";
import { ContextMenu, cn, type MenuItem, Tooltip, toast } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import {
  type ButtonHTMLAttributes,
  type CSSProperties,
  type ReactNode,
  useEffect,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { type PlanDay, planWhen } from "./crew-plans";
import { inside, leaveOnEscape } from "./edit-escape";
import { openMenuFromKey } from "./menu-key";
import { pop, useSocialMotion } from "./motion";
import { cleanPlanNote, planNoteProblem } from "./plan-note";
import {
  builtInTemplates,
  cleanTemplateName,
  type PlanTemplate,
  TEMPLATE_NAME_MAX,
  templateFrom,
  templateTime,
  usePlanTemplates,
} from "./plan-templates";

/** The chips' cut, the day chips' plate's (`PLATE_CUT` in @pl/ui). */
const CUT = { "--cut": "5px" } as CSSProperties;

/**
 * One chip: the day chips' size and words, its own plate, the focus ring outside its cut. A menu
 * or a tooltip around it passes its own handlers (and the ref) through, kept alongside `onPress`.
 */
function TemplateChip({
  label,
  on,
  icon,
  quiet = false,
  onPress,
  onClick,
  ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  label: string;
  /** The form holds its fill now: the quiet plate, as the chosen day. */
  on?: boolean;
  icon?: ReactNode;
  /** "Şablonlara Ekle": an action rather than a fill, a step quieter. */
  quiet?: boolean;
  onPress: () => void;
}) {
  return (
    <button
      type="button"
      {...rest}
      style={CUT}
      aria-pressed={on}
      onClick={(event) => {
        onClick?.(event);
        onPress();
      }}
      className={cn(
        "group/chip relative isolate flex h-[26px] max-w-full shrink-0 cursor-pointer items-center gap-1.5 px-3 text-[13px] font-medium whitespace-nowrap transition-[color,transform] duration-150 ease-out focus-visible:outline-none active:scale-[0.97]",
        on ? "text-bone-50" : "text-ash-300 hover:text-bone-50",
      )}
    >
      <span
        aria-hidden
        className={cn(
          "chamfer pointer-events-none absolute inset-0 -z-10 transition-colors duration-150",
          on
            ? "bg-[color:var(--segmented-plate)]"
            : quiet
              ? "bg-white/[0.03] group-hover/chip:bg-white/[0.07]"
              : "bg-white/[0.06] group-hover/chip:bg-white/[0.1]",
        )}
      />
      <span
        aria-hidden
        className="chamfer-ring pointer-events-none absolute -z-10 bg-gold-300 opacity-0 group-focus-visible/chip:opacity-100"
      />
      {icon}
      <span className="min-w-0 truncate">{label}</span>
    </button>
  );
}

/** What a chip fills, resting on it: the day and time, the note under them. */
function FillTip({ when, note }: { when: string; note: string }) {
  return (
    <span className="flex flex-col gap-0.5">
      <span>{when}</span>
      {note ? <span className="text-ash-300">{note}</span> : null}
    </span>
  );
}

/** A small action inside the rename field (✓, ×). */
function FieldAction({
  label,
  icon,
  onPress,
}: {
  label: string;
  icon: ReactNode;
  onPress: () => void;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      onClick={onPress}
      className="flex size-5 shrink-0 cursor-pointer items-center justify-center rounded-[4px] text-ash-300 transition-[color,background-color,transform] duration-150 ease-out hover:bg-white/10 hover:text-bone-50 focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none active:scale-[0.97]"
    >
      {icon}
    </button>
  );
}

/**
 * Renaming one of the player's own in its chip's place (the other chips stay, the row keeps its
 * height): Enter keeps the name, Esc leaves it as it was.
 */
function RenameField({
  initial,
  onDone,
  onCancel,
}: {
  initial: string;
  onDone: (name: string) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [value, setValue] = useState(initial);
  const input = useRef<HTMLInputElement>(null);
  const box = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    input.current?.focus();
    input.current?.select();
  }, []);
  // Esc gives up the name, never the panel around it (edit-escape.ts).
  const cancel = useRef(onCancel);
  cancel.current = onCancel;
  useEffect(
    () =>
      leaveOnEscape(
        (target) => inside(box.current, target),
        () => cancel.current(),
      ),
    [],
  );
  const save = () => {
    const name = cleanTemplateName(value);
    if (name) onDone(name);
    else onCancel();
  };
  return (
    <span ref={box} className="relative flex h-[26px] max-w-full items-center">
      <input
        ref={input}
        value={value}
        maxLength={TEMPLATE_NAME_MAX * 2}
        aria-label={t("crewPlus.templates.name")}
        placeholder={t("crewPlus.templates.name")}
        onChange={(event) => setValue(event.target.value)}
        onKeyDown={(event) => {
          // The plan form around it must not take Enter as "Planla".
          if (event.key === "Enter") {
            event.preventDefault();
            save();
          }
        }}
        className="field-sizing-content h-[26px] max-w-[13rem] min-w-[7rem] rounded-md border border-white/10 bg-obsidian-800/80 pr-[48px] pl-2.5 text-[13px] text-bone-50 transition-[border-color,box-shadow] duration-150 outline-none placeholder:text-ash-500 hover:border-white/16 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-300/25"
      />
      {/* ✓ and × inside the field's end: the field takes about the chip's room, the row stays. */}
      <span className="absolute top-1/2 right-1 flex -translate-y-1/2 gap-0.5">
        <FieldAction
          label={t("crewPlus.templates.renameSave")}
          icon={<Check size={12} weight="bold" aria-hidden />}
          onPress={save}
        />
        <FieldAction
          label={t("crewPlus.templates.renameCancel")}
          icon={<X size={12} weight="bold" aria-hidden />}
          onPress={onCancel}
        />
      </span>
    </span>
  );
}

/**
 * The template chips over the plan form. `at` and `note` are the form's now; `opened` its clock
 * (the days and times it offers); `notes`: the service keeps plan notes (else a template fills the
 * day and time only). `onFill` puts a fill in the form; `onRefuse` says in the form's line why
 * "Şablonlara Ekle" kept nothing.
 */
export function TemplateChips({
  days,
  day,
  opened,
  at,
  note,
  notes,
  onFill,
  onRefuse,
  onRenaming,
}: {
  days: readonly PlanDay[];
  day: PlanDay;
  opened: number;
  at: number;
  note: string;
  notes: boolean;
  onFill: (at: number, note: string) => void;
  onRefuse: (text: string) => void;
  /** A name is being written: the form's own buttons wait meanwhile. */
  onRenaming?: (renaming: boolean) => void;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const own = usePlanTemplates((s) => s.own);
  const [renaming, setRenamingId] = useState<string | null>(null);
  const setRenaming = (id: string | null) => {
    setRenamingId(id);
    onRenaming?.(id !== null);
  };
  // The template went while its name was being written (deleted, or a backup came): the field goes.
  useEffect(() => {
    if (renaming !== null && !own.some((one) => one.id === renaming)) {
      setRenamingId(null);
      onRenaming?.(false);
    }
  }, [own, renaming, onRenaming]);
  const formNote = cleanPlanNote(note);

  const fillOf = (template: PlanTemplate) => templateTime(template, days, opened);
  const holds = (template: PlanTemplate) =>
    fillOf(template) === at && (!notes || formNote === template.note);
  const fill = (template: PlanTemplate) => {
    const next = fillOf(template);
    if (next === null) return;
    onFill(next, notes ? template.note : "");
    announce(
      t("crewPlus.templates.fill", { name: template.name, when: planWhen(next, Date.now(), t) }),
    );
  };
  const tip = (template: PlanTemplate) => {
    const next = fillOf(template);
    return next === null ? null : (
      <FillTip when={planWhen(next, Date.now(), t)} note={notes ? template.note : ""} />
    );
  };
  const save = () => {
    // A note that could not go (its reason shows under it) is no template either, as "Planla" waits.
    if (notes && planNoteProblem(note)) return;
    const outcome = usePlanTemplates
      .getState()
      .add(templateFrom(at, day, notes ? note : "", Date.now(), t));
    if (outcome === "added") announce(t("crewPlus.templates.saved"));
    else onRefuse(t(outcome === "full" ? "crewPlus.templates.full" : "crewPlus.templates.exists"));
  };
  const remove = (id: string) => {
    const store = usePlanTemplates.getState();
    const gone = store.remove(id);
    if (!gone) return;
    toast(t("crewPlus.templates.removed"), {
      action: {
        label: t("crewPlus.templates.undo"),
        onClick: () => usePlanTemplates.getState().restore(gone.template, gone.index),
      },
    });
  };

  return (
    <fieldset
      aria-label={t("crewPlus.templates.label")}
      className="mb-3 flex min-w-0 flex-wrap gap-1.5"
    >
      {builtInTemplates(t).map((template) => (
        <Tooltip key={template.key} content={tip(template)} disabled={tip(template) === null}>
          <TemplateChip label={template.name} on={holds(template)} onPress={() => fill(template)} />
        </Tooltip>
      ))}
      <AnimatePresence initial={false} mode="popLayout">
        {own.map((template) => {
          const items: MenuItem[] = [
            {
              label: t("crewPlus.templates.rename"),
              icon: <PencilSimple size={16} />,
              // Once the menu is gone: the name field takes the focus.
              onSelect: () => window.setTimeout(() => setRenaming(template.id), 0),
            },
            {
              label: t("crewPlus.templates.remove"),
              icon: <Trash size={16} />,
              danger: true,
              onSelect: () => remove(template.id),
            },
          ];
          if (template.id === renaming) {
            return (
              <motion.span key={template.id} className="flex max-w-full" {...pop(reduce)}>
                <RenameField
                  initial={template.name}
                  onDone={(name) => {
                    usePlanTemplates.getState().rename(template.id, name);
                    setRenaming(null);
                  }}
                  onCancel={() => setRenaming(null)}
                />
              </motion.span>
            );
          }
          return (
            <motion.span key={template.id} className="flex max-w-full" {...pop(reduce)}>
              <ContextMenu
                items={items}
                title={template.name}
                wrapTrigger={(trigger) => (
                  <Tooltip content={tip(template)} disabled={tip(template) === null}>
                    {trigger}
                  </Tooltip>
                )}
              >
                <TemplateChip
                  label={template.name}
                  on={holds(template)}
                  onPress={() => fill(template)}
                  // Shift+F10 and the menu key open its menu under it (menu-key.ts).
                  onKeyDown={(event) => void openMenuFromKey(event)}
                />
              </ContextMenu>
            </motion.span>
          );
        })}
      </AnimatePresence>
      <TemplateChip
        label={t("crewPlus.templates.save")}
        quiet
        icon={<Plus size={12} weight="bold" aria-hidden className="shrink-0" />}
        onPress={save}
      />
    </fieldset>
  );
}
