// The room panel's two shells (SQOL-U3): over the page as a sheet (a dialog: its title and close
// are the dialog's), or docked beside the stream's stage (a complementary landmark, "Ses Odası":
// its title a heading, its close a plain button). The pages inside take their title and close
// from here, so they are the same in both.
import { X } from "@phosphor-icons/react";
import { IconButton } from "@pl/ui";
import { Dialog } from "radix-ui";
import { createContext, type ReactNode, useContext } from "react";
import { useTranslation } from "react-i18next";
import type { PanelLayout } from "./panel-size";

export interface PanelChrome {
  /** Docked beside the stage (no scrim, the page stays live). */
  docked: boolean;
  /** The panel is on screen (not sliding away, not hidden behind a full-screen stage). */
  shown: boolean;
  close: () => void;
  /** How its width lays it out (panel-size.ts): one column, or the people beside the chat. */
  layout: PanelLayout;
}

const PanelChromeContext = createContext<PanelChrome>({
  docked: false,
  shown: false,
  close: () => {},
  layout: "stacked",
});

export const PanelChromeProvider = PanelChromeContext.Provider;

export function usePanelChrome(): PanelChrome {
  return useContext(PanelChromeContext);
}

/** The panel's title: the dialog's own while it is a sheet, a heading while it is docked. */
export function PanelTitle({ className, children }: { className?: string; children: ReactNode }) {
  const { docked } = usePanelChrome();
  return docked ? (
    <h2 className={className}>{children}</h2>
  ) : (
    <Dialog.Title className={className}>{children}</Dialog.Title>
  );
}

/** The panel's "Kapat". */
export function PanelClose() {
  const { t } = useTranslation();
  const { docked, close } = usePanelChrome();
  const button = <IconButton label={t("common.close")} icon={<X size={18} />} />;
  return docked ? (
    <IconButton label={t("common.close")} icon={<X size={18} />} onClick={close} />
  ) : (
    <Dialog.Close asChild>{button}</Dialog.Close>
  );
}
