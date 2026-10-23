// "Dünyan açık" at the top of the friends panel while the player's world is shared (HOST1,
// host1.md §6.1): the world, how many are in it, the faces of the friends in it (a newcomer's face
// grows in where it stays, the others glide aside) and "Yönet", which opens the world's sheet in
// the panel's place. The whole strip is one button. While the world opens it says where it is;
// when it could not open, so.
import { cn } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { rise, useSocialMotion } from "@/features/social/motion";
import { useSocialUi } from "@/features/social/store";
import type { HostSession } from "@/lib/api/host-types";
import { Faces, PeopleCount, useWorldIcon, WorldMark } from "./parts";
import { sessionPeople } from "./rules";
import { useHostUi } from "./store";

/** The friends panel gives way to the world's sheet (one right sheet at a time). */
export function openWorldSheet() {
  useSocialUi.getState().closePanel();
  useHostUi.getState().openSheet();
}

function Line({ session }: { session: HostSession }) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const people = sessionPeople(session);
  const title =
    session.phase === "open"
      ? t("host.strip.open")
      : session.phase === "opening"
        ? t("host.strip.opening")
        : t("host.strip.failed");
  return (
    <span className="relative block min-w-0 flex-1 text-left">
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span key={session.phase} className="block" {...rise(reduce, 4, 0.2)}>
          <span className="flex items-center gap-2">
            <span
              className={cn(
                "truncate text-[13.5px] font-semibold",
                session.phase === "open"
                  ? "text-jade-400"
                  : session.phase === "failed"
                    ? "text-flare-300"
                    : "text-gold-200",
              )}
            >
              {title}
            </span>
            {session.phase === "open" ? (
              <PeopleCount
                count={people.count}
                max={people.max}
                label={t("host.sheet.countLabel", people)}
                className="text-[12.5px] text-ash-300"
              />
            ) : null}
          </span>
          <span className="block truncate text-[12.5px] text-ash-300">{session.worldName}</span>
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

/** The world's own icon (the grass block when the library has none). */
function StripMark({ session }: { session: HostSession }) {
  const icon = useWorldIcon(session);
  return <WorldMark size={40} iconUrl={icon} quiet />;
}

export function WorldStrip() {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const enabled = useHostUi((s) => s.view.enabled);
  const session = useHostUi((s) => s.view.session);
  const shown = enabled ? session : null;
  const inWorld = (shown?.guests ?? [])
    .filter((g) => g.state === "in")
    .map((g) => ({ ...g, id: g.friend }));
  return (
    <AnimatePresence initial={false}>
      {shown ? (
        <motion.div key="world" className="mb-2" {...rise(reduce, 4)}>
          <button
            type="button"
            onClick={openWorldSheet}
            className="group flex w-full cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] bg-white/[0.04] py-2 pr-2 pl-2 text-left ring-1 ring-white/8 outline-none transition-[background-color,box-shadow] duration-150 hover:bg-white/[0.065] focus-visible:ring-2 focus-visible:ring-gold-300"
          >
            <StripMark session={shown} />
            <Line session={shown} />
            {inWorld.length ? <Faces sources={inWorld} size={24} /> : null}
            {/* The strip's action, drawn inside its one button (as Home's lines do). */}
            <span className="relative isolate inline-flex h-8 shrink-0 items-center px-3 text-[12.5px] font-medium whitespace-nowrap text-bone-100 transition-[transform,scale] duration-150 ease-out [--cut:6px] group-active:scale-[0.97]">
              <span
                aria-hidden
                className="chamfer absolute inset-0 -z-10 bg-white/12 transition-colors group-hover:bg-white/20"
              />
              <span
                aria-hidden
                className="chamfer absolute inset-px -z-10 bg-obsidian-800/85 transition-colors group-hover:bg-obsidian-700/90"
              />
              {t("host.strip.manage")}
            </span>
          </button>
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
