import { cn, DropZone } from "@pl/ui";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { type CopyKind, planDrop } from "./model";
import { useDropAnnouncement, useDropZone } from "./store";
import { toTarget, useDropPlan, useDropTarget } from "./target";
import { announcementOf, windowHint, zoneTitle } from "./text";

/**
 * "Buraya bırak" in an empty tab (mods, resource packs, shaders): the place to drop that kind of
 * file, in plain sight. Files dropped anywhere on the window still go to the instance as before
 * (DropHost does the work); while they are dragged this zone takes the window zone's place and
 * reacts: it lights up under the cursor and says what will happen.
 */
export function PageDropZone({
  kind,
  label,
  actions,
  className,
}: {
  /** What the tab holds: the line at rest names that kind of file. */
  kind: CopyKind;
  /** The empty state's title, for screen readers ("Bu kurulumda mod yok"). */
  label: string;
  /** Buttons that do the same without dragging (pick a file, browse). */
  actions?: ReactNode;
  className?: string;
}) {
  const { t } = useTranslation();
  const target = useDropTarget();
  const zone = useDropZone({
    layer: "page",
    accepts: (drag) => (drag.files ? planDrop(drag.files, toTarget(target)).acceptable : true),
  });
  const plan = useDropPlan(zone.drag?.files, target);
  const dragging = zone.drag !== null;
  const title = zoneTitle(t, zone.state, zone.drag?.paths.length ?? 1);
  const rest = t(`drop.rest.${kind}`);
  // While the files are read (milliseconds) the line at rest stays, then it changes in place.
  const hint = dragging ? (windowHint(t, plan, target?.name, true) ?? rest) : rest;
  useDropAnnouncement(
    dragging && zone.state !== "absorb" && plan ? announcementOf(zone.state, title, hint) : null,
    zone.drag?.id,
  );
  return (
    // Room around the zone for its brackets, so they never widen the tab's scroll area.
    <div className={cn("p-[12px]", className)}>
      <p className="sr-only">{label}</p>
      <DropZone
        ref={zone.ref}
        state={zone.state}
        dimmed={zone.dimmed}
        title={title}
        hint={hint}
        className="min-h-[240px]"
      >
        {actions}
      </DropZone>
    </div>
  );
}
