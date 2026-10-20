import { cn, Switch } from "@pl/ui";
import { type ComponentProps, cloneElement, isValidElement, type ReactNode, useId } from "react";

/**
 * One setting: its title and help on the left, its control on the right (a longer label grows
 * left, never wraps the control). `helpId` names the help line, so a control other than a switch
 * can be described by it.
 */
export function Row({
  title,
  help,
  helpId: helpIdProp,
  children,
}: {
  title: string;
  help?: ReactNode;
  helpId?: string;
  children: ReactNode;
}) {
  const id = useId();
  const helpId = helpIdProp ?? `${id}-help`;
  // A row whose control is a switch also toggles on a click on its words: its title and help
  // become the switch's label (a target as wide as the row, POLISH2). The switch is named by
  // the title and described by the help (POLISH3: a screen reader read the title only).
  const toggle =
    isValidElement<ComponentProps<typeof Switch>>(children) && children.type === Switch;
  const control = toggle
    ? cloneElement(children, { id, describedBy: help ? helpId : undefined })
    : children;
  const words = (
    <>
      <span className="block text-[13.5px] font-medium text-bone-50">{title}</span>
      {help ? (
        <span id={helpId} className="mt-0.5 block max-w-[56ch] text-[12.5px] text-ash-300">
          {help}
        </span>
      ) : null}
    </>
  );
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-6 border-b border-white/6 py-3.5 last:border-b-0",
        // The switch lifts while the pointer is on its words too (`hovered:`).
        toggle && "switch-row",
      )}
    >
      {toggle ? (
        <label
          htmlFor={id}
          className={cn("min-w-0", children.props.disabled ? "cursor-default" : "cursor-pointer")}
        >
          {words}
        </label>
      ) : (
        <div className="min-w-0">{words}</div>
      )}
      <div className="shrink-0">{control}</div>
    </div>
  );
}
