import { Button } from "@pl/ui";
import { Popover } from "radix-ui";
import { type ReactNode, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { type AskFrom, useUpdate } from "@/stores/update";
import { cancelInstall, confirmInstall } from "./install";

/**
 * Wraps a "restart and update" control (the title bar pill, the Settings button). When AFK
 * accounts are open, pressing it first opens this popover at the control: one sentence (what
 * happens to the accounts is not obvious), "Güncelle" and "Sonra".
 */
export function InstallConfirm({ from, children }: { from: AskFrom; children: ReactNode }) {
  const { t } = useTranslation();
  const asking = useUpdate((s) => (s.asking?.from === from ? s.asking : null));
  const panel = useRef<HTMLDivElement>(null);
  // The sentence keeps its wording while the popover fades out after "Güncelle".
  const [count, setCount] = useState(1);
  if (asking && asking.count !== count) setCount(asking.count);

  // A press on the control itself is not "outside": the control closes the popover (a toggle).
  const onInteractOutside = (event: {
    detail: { originalEvent: Event };
    preventDefault(): void;
  }) => {
    const target = event.detail.originalEvent.target;
    if (target instanceof Element && target.closest(`[data-update-anchor="${from}"]`)) {
      event.preventDefault();
    }
  };

  return (
    <Popover.Root
      open={asking !== null}
      onOpenChange={(open) => {
        if (!open) cancelInstall();
      }}
    >
      <Popover.Anchor asChild data-update-anchor={from}>
        {children}
      </Popover.Anchor>
      <Popover.Portal>
        <Popover.Content
          ref={panel}
          side="bottom"
          align="end"
          sideOffset={8}
          collisionPadding={12}
          tabIndex={-1}
          // The panel takes the focus, not "Güncelle": Enter must not stop the AFK accounts.
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            panel.current?.focus({ preventScroll: true });
          }}
          onInteractOutside={onInteractOutside}
          className="glass-strong pop-motion z-50 w-[280px] rounded-[var(--radius-panel)] p-3.5 shadow-[0_24px_60px_rgb(34_6_6/0.55)] outline-none"
        >
          <p className="text-[13px] leading-snug text-bone-100">
            {t("updates.afkComeBack", { count })}
          </p>
          <div className="mt-3 flex justify-end gap-2">
            <Button size="sm" variant="ghost" onClick={cancelInstall}>
              {t("updates.later")}
            </Button>
            <Button size="sm" variant="primary" onClick={confirmInstall}>
              {t("updates.install")}
            </Button>
          </div>
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  );
}
