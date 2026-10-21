import { Tooltip as RadixTooltip } from "radix-ui";
import {
  cloneElement,
  type FocusEvent,
  type PointerEvent,
  type ReactElement,
  type ReactNode,
  type RefObject,
  useState,
} from "react";

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

/** A trigger whose pointer and focus handlers the tooltip below adds to. */
type TriggerProps = {
  onPointerEnter?: (event: PointerEvent<HTMLElement>) => void;
  onFocus?: (event: FocusEvent<HTMLElement>) => void;
};

/** Cut with an ellipsis: wider than its box by more than a rounding pixel. */
export function isCut(element: Pick<HTMLElement, "scrollWidth" | "clientWidth"> | null): boolean {
  return element !== null && element.scrollWidth - element.clientWidth > 1;
}

/**
 * The whole of a text that is cut with an ellipsis (a long instance name in a narrow place), in a
 * tooltip; none while the text fits, since a tooltip repeating the words on screen is noise. `of`
 * is the element that truncates. It is measured each time the pointer or the focus reaches the
 * trigger, so a resized window or a renamed instance is always current, and nothing is watched in
 * between. The focus opens it as well (the keyboard, and the rows of a menu, which take the focus
 * under the pointer).
 */
export function OverflowTooltip({
  text,
  of,
  side,
  children,
}: {
  text: string;
  of: RefObject<HTMLElement | null>;
  side?: "top" | "right" | "bottom" | "left";
  children: ReactElement<TriggerProps>;
}) {
  const [cut, setCut] = useState(false);
  const measure = () => setCut(text !== "" && isCut(of.current));
  const own = children.props;
  const trigger = cloneElement(children, {
    onPointerEnter: (event: PointerEvent<HTMLElement>) => {
      own.onPointerEnter?.(event);
      measure();
    },
    onFocus: (event: FocusEvent<HTMLElement>) => {
      own.onFocus?.(event);
      measure();
    },
  });
  return (
    <Tooltip content={text} side={side} disabled={!cut}>
      {trigger}
    </Tooltip>
  );
}
