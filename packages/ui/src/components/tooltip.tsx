import { Tooltip as RadixTooltip } from "radix-ui";
import { type ReactElement, type ReactNode, useState } from "react";

export function TooltipProvider({ children }: { children: ReactNode }) {
  return (
    <RadixTooltip.Provider delayDuration={350} skipDelayDuration={150}>
      {children}
    </RadixTooltip.Provider>
  );
}

export function Tooltip({
  content,
  side = "top",
  disabled,
  children,
}: {
  content: ReactNode;
  side?: "top" | "right" | "bottom" | "left";
  /**
   * The tooltip does not open (a reason shown only while its control waits). Wrapping the
   * control conditionally instead would mount a new control each time, and its transitions (a
   * button fading into its disabled look) would never play.
   */
  disabled?: boolean;
  children: ReactElement;
}) {
  const [open, setOpen] = useState(false);
  // Controlled only where it can be turned off, so every other tooltip stays as it was.
  const control = disabled === undefined ? {} : { open: open && !disabled, onOpenChange: setOpen };
  return (
    <RadixTooltip.Root {...control}>
      <RadixTooltip.Trigger asChild>{children}</RadixTooltip.Trigger>
      <RadixTooltip.Portal>
        <RadixTooltip.Content
          side={side}
          sideOffset={6}
          className="glass-strong pop-motion z-50 rounded-md px-2 py-1 text-[12px] font-medium text-bone-100 shadow-[0_12px_32px_rgb(34_6_6/0.45)]"
        >
          {content}
        </RadixTooltip.Content>
      </RadixTooltip.Portal>
    </RadixTooltip.Root>
  );
}
