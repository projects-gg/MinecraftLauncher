// The account panel's Envanter tab (CAFK-FEAT §3.1): the account's inventory as in the game, live
// while the tab is on screen, moved by hand with the game's own clicks (`@pl/ui/inventory`). The
// items are the game's own pictures from the player's installed Minecraft (ITEMART), with the
// Projects server's own looks and menu pictures on its servers (IA-ART). It sits on a solid panel:
// inside glass Chromium resamples its pixels at 125 % (docs/design.md §6 "Cloud inventory").
import { Button, ContentIcon, cn, EmptyState } from "@pl/ui";
import { InventoryView, type TitleGlyph } from "@pl/ui/inventory";
import { hostMatches, type ItemArt, ItemArtProvider } from "@pl/ui/items";
import { useQueryClient } from "@tanstack/react-query";
import { type ReactNode, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { type ItemOverlay, itemArtOverlay, vanillaItemArt } from "@/lib/item-art";
import { afkFailed } from "../../errors";
import { type CloudAccount, cloudApi } from "../api";
import { serverLabel } from "../status";
import { storeAccount } from "../use-cloud";
import { useCloudInventory } from "./use-cloud-inventory";

/** Pictures already drawn, by Minecraft version: a panel opened again has them at its first frame. */
const known = new Map<string, ItemArt>();
let knownOverlay: ItemOverlay | undefined;

/**
 * The item pictures for an account: the vanilla atlas of its Minecraft version (else the newest
 * installed one) and, on the servers it names, the Projects overlay. Started when the panel
 * opens, so the tab rarely shows its stand-ins.
 */
export function useInventoryArt(account: CloudAccount | undefined) {
  const version = account?.version && account.version !== "auto" ? account.version : undefined;
  const key = version ?? "";
  const address = account ? serverLabel(account.server) : undefined;
  const [vanilla, setVanilla] = useState(known.get(key)?.vanilla);
  const [overlay, setOverlay] = useState(knownOverlay);
  const wanted = Boolean(account && (account.features?.inventory?.mode ?? "off") !== "off");
  useEffect(() => {
    if (!wanted) return;
    let live = true;
    setVanilla(known.get(key)?.vanilla);
    void vanillaItemArt(version).then((atlas) => {
      if (!atlas) return;
      known.set(key, { vanilla: atlas });
      if (live) setVanilla(atlas);
    });
    void itemArtOverlay().then((loaded) => {
      if (!loaded) return;
      knownOverlay = loaded;
      if (live) setOverlay(loaded);
    });
    return () => {
      live = false;
    };
  }, [wanted, key, version]);
  const onItsServer = Boolean(overlay && hostMatches(address, overlay.hosts));
  const art: ItemArt | undefined =
    vanilla || onItsServer
      ? {
          vanilla,
          overlay: onItsServer ? overlay?.overlay : undefined,
          host: onItsServer ? address : undefined,
        }
      : undefined;
  const titleGlyph: ((codePoint: number) => TitleGlyph | undefined) | undefined =
    onItsServer && overlay ? overlay.glyph : undefined;
  return { art, titleGlyph };
}

function Quiet({ text, action }: { text: string; action?: ReactNode }) {
  return (
    <EmptyState
      icon={<ContentIcon kind="inventory" size={32} />}
      title={text}
      action={action}
      className="py-14"
    />
  );
}

export function InventoryPanel({
  account,
  active,
  link,
  art,
  titleGlyph,
  onReconnect,
  onOpenSettings,
  onUnavailable,
}: {
  account: CloudAccount;
  /** The tab is on screen: the inventory streams only then. */
  active: boolean;
  /** The console's reconnect count: the stream is asked for again with it. */
  link: number;
  art: ItemArt | undefined;
  titleGlyph?: (codePoint: number) => TitleGlyph | undefined;
  onReconnect: () => void;
  onOpenSettings: () => void;
  /** The service cannot show it after all (Studio switched it off): the tab goes. */
  onUnavailable: () => void;
}) {
  const { t, i18n } = useTranslation();
  const queryClient = useQueryClient();
  const { state, onOp, mode } = useCloudInventory(account, active, link);
  const [turningOn, setTurningOn] = useState(false);
  const [fix, setFix] = useState(false);
  const lang = i18n.language === "en" ? "en" : "tr";
  const on = account.features?.inventory?.on ?? false;
  const inGame = account.live.state === "running";

  useEffect(() => {
    if (state.status === "off" && state.reason === "service") onUnavailable();
  }, [state.status, state.reason, onUnavailable]);

  const turnOn = async () => {
    setTurningOn(true);
    setFix(false);
    try {
      const outcome = await cloudApi.setInventory(account.id, true);
      if (outcome.kind === "saved") storeAccount(queryClient, outcome.account);
      else if (outcome.kind === "invalid") setFix(true);
      else storeAccount(queryClient, outcome.account);
    } catch (error) {
      afkFailed(error, t("cloudAfkFeat.inventory.turnOnFailed"));
    } finally {
      setTurningOn(false);
    }
  };

  // What holds the view, from the account first (no need to wait for the machine), then the
  // machine's word.
  const reason = !on
    ? "setting"
    : state.status === "off"
      ? state.reason
      : !inGame && state.status !== "ok"
        ? "not_running"
        : null;

  if (reason === "setting") {
    return (
      <Quiet
        text={t("cloudAfkFeat.inventory.off")}
        action={
          <div className="flex flex-col items-center gap-2">
            <Button size="sm" variant="primary" disabled={turningOn} onClick={() => void turnOn()}>
              {t("cloudAfkFeat.inventory.turnOn")}
            </Button>
            {fix ? (
              <p role="alert" className="text-[12.5px] text-flare-400">
                {t("cloudAfkFeat.inventory.fixSettings")}{" "}
                <button
                  type="button"
                  onClick={onOpenSettings}
                  className="cursor-pointer font-medium text-gold-300 underline-offset-2 outline-none hover:underline focus-visible:ring-2 focus-visible:ring-gold-300"
                >
                  {t("cloudAfkFeat.inventory.openSettings")}
                </button>
              </p>
            ) : null}
          </div>
        }
      />
    );
  }
  if (reason === "restart") {
    return (
      <Quiet
        text={t("cloudAfkFeat.inventory.restart")}
        action={
          <Button size="sm" variant="secondary" onClick={onReconnect}>
            {t("cloudAfkFeat.inventory.reconnect")}
          </Button>
        }
      />
    );
  }
  if (reason === "version") return <Quiet text={t("cloudAfkFeat.inventory.version")} />;
  if (reason === "not_running") return <Quiet text={t("cloudAfkFeat.inventory.notRunning")} />;
  // `service` closes the tab (above); a reason this launcher does not know reads as the general one.
  if (reason) return <Quiet text={t("cloudAfkFeat.inventory.unavailable")} />;

  return (
    <div className="flex flex-col gap-3 pb-2">
      <ItemArtProvider value={art}>
        {/* The view is a size container: it needs the width (it centres its own art). */}
        <InventoryView
          // Until the first frame the grid is a stand-in: dimmed, so it never reads as empty.
          className={cn(
            "w-full transition-opacity duration-200",
            state.status === "waiting" && "opacity-55",
          )}
          inventory={state.inventory}
          window={state.window}
          mode={mode === "view" ? "view" : "full"}
          locked={state.locked}
          onOp={onOp}
          lang={lang}
          titleGlyph={titleGlyph}
          maxScale={3}
        />
      </ItemArtProvider>
      {/* Until the first frame the empty grid is a stand-in: the line says it is coming. */}
      <p role="status" className="text-center text-[12px] text-ash-400">
        {state.status === "waiting"
          ? t("cloudAfkFeat.inventory.loading")
          : t("cloudAfkFeat.inventory.privacy")}
      </p>
    </div>
  );
}
