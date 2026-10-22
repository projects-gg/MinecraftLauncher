// A person's name in the chat (a crew message's sender line, the people who reacted): the one
// place it is drawn, so that a crew tag (TEAM6) can sit before it later. Menus may take it as
// their trigger: its ref and props go to the span.
import { cn } from "@pl/ui";
import type { ComponentPropsWithRef } from "react";

export function SenderName({
  name,
  className,
  ...rest
}: { name: string } & Omit<ComponentPropsWithRef<"span">, "children">) {
  return (
    <span {...rest} className={cn("min-w-0 truncate", className)}>
      {name}
    </span>
  );
}
