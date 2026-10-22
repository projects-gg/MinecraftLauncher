// A person's name in the chat (a crew message's sender line, the people who reacted): the one
// place it is drawn, with the crew tag they wear before it (TEAM6; its card opens under the mouse,
// a press goes to the name). Menus may take it as their trigger: its ref and props go to the span.
import { cn } from "@pl/ui";
import type { ComponentPropsWithRef } from "react";
import type { CrewBadge, SocialProfile } from "@/lib/api/social-types";
import { BadgedName } from "../crew-badge";

/** Who wrote something, as the lists know them (the tag they wear, if any). */
export type ChatPerson = Pick<SocialProfile, "name" | "avatar" | "avatarUrl"> & {
  badge?: CrewBadge;
};

export function SenderName({
  name,
  badge,
  wearer,
  className,
  ...rest
}: {
  name: string;
  /** The tag they wear (shown only with `wearer`, their id). */
  badge?: CrewBadge;
  wearer?: string;
} & Omit<ComponentPropsWithRef<"span">, "children">) {
  return (
    <span {...rest} className={cn("flex min-w-0", className)}>
      <BadgedName
        name={name}
        badge={wearer ? badge : undefined}
        wearer={wearer ?? ""}
        size="sm"
        className="gap-1"
      />
    </span>
  );
}
