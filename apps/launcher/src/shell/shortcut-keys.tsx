import { cn, Kbd } from "@pl/ui";
import { detectOs } from "@/lib/platform";
import { type ShortcutId, shortcutKeys } from "@/lib/shortcuts";

/** Key caps, one per key ("Ctrl", "1"), quiet beside a label: palette rows, tooltips. */
export function KeyCaps({ keys, className }: { keys: readonly string[]; className?: string }) {
  return (
    <kbd className={cn("inline-flex shrink-0 items-center gap-[3px] font-sans", className)}>
      {keys.map((key) => (
        <Kbd key={key}>{key}</Kbd>
      ))}
    </kbd>
  );
}

/** A global shortcut's key caps on this computer (⌘ on macOS). */
export function ShortcutKeys({ id, className }: { id: ShortcutId; className?: string }) {
  return <KeyCaps keys={shortcutKeys(id, detectOs())} className={className} />;
}
