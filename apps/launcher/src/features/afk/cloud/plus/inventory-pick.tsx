// "Envanterden seç" (afkplus-free.md §7.2): the bot's own inventory as in the game, live while the
// dialog is open (the cloud inventory's stream, FEAT-L). A click, a tap or Enter on one of its items
// chooses it; "Bunu seç" asks the machine what it is (`POST …/plus/items/pick`): a plain vanilla
// item gives its id and its enchantments and durability as an order's conditions, any other a
// custom item with its fingerprint (one of the account's own when it was named before, else a new
// one the order names). Nothing moves. A solid panel: inside glass Chromium resamples the game's
// pixels (docs/design.md §6 "Cloud inventory"). It opens over the order form, so above its sheet.
import { X } from "@phosphor-icons/react";
import {
  Button,
  ContentIcon,
  cn,
  dialogMotion,
  EntranceGate,
  enter,
  focusPanel,
  GatedMotion,
  IconButton,
  leave,
  scrimMotion,
  useInteractionMotion,
} from "@pl/ui";
import { InventoryView } from "@pl/ui/inventory";
import { AnimatePresence, motion } from "motion/react";
import { Dialog as RadixDialog } from "radix-ui";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { isTauri } from "@/lib/platform";
import { afkFailed } from "../../errors";
import type { CloudAccount } from "../api";
import { useCloudInventory } from "../inventory/use-cloud-inventory";
import { type PlusCustomItem, type PlusPick, plusApi } from "./api";
import { usePlusTitleGlyph } from "./item-icon";
import { mockPickInventory } from "./mock-data";
import type { CustomItemLike } from "./order-draft";
import { newCustomKey } from "./orders";
import { currentLang, goodsText } from "./words";

/** The conditions a vanilla item suggests (its enchantments and durability). */
export interface PickedReq {
  ench: Record<string, number>;
  dur: number | null;
  named: boolean;
  cursed: boolean;
}

/**
 * What a pick gives: a vanilla item, one of the account's custom items (known by its
 * fingerprint), or a new custom item (`name` empty when the item shows none: the form asks).
 */
export type PickedItem =
  | { kind: "vanilla"; ref: string; req: PickedReq }
  | { kind: "known"; ref: string }
  | { kind: "custom"; custom: CustomItemLike };

type CustomLike = Pick<PlusCustomItem, "k" | "fp">;

/** The pick as the order takes it. */
export function pickedOf(pick: PlusPick, customItems: readonly CustomLike[]): PickedItem | null {
  if (pick.custom) {
    const fp = pick.custom.fp;
    const found = customItems.find((item) => item.fp && item.fp === fp);
    if (found) return { kind: "known", ref: `custom:${found.k}` };
    const look = pick.custom.look;
    return {
      kind: "custom",
      custom: {
        k: newCustomKey(customItems),
        name: pick.custom.name ?? "",
        base: pick.custom.base,
        fp,
        look:
          look && (look.cmd != null || look.model != null)
            ? { cmd: look.cmd ?? undefined, model: look.model ?? undefined }
            : null,
        from: "inv",
      },
    };
  }
  if (!pick.ref) return null;
  return {
    kind: "vanilla",
    ref: pick.ref,
    req: {
      ench: pick.req?.ench ?? {},
      dur: pick.req?.dur ?? null,
      named: pick.req?.named ?? false,
      cursed: pick.req?.cursed ?? false,
    },
  };
}

const FAILED = new Set([
  "empty",
  "stale",
  "unparsed",
  "container",
  "not_in_game",
  "inventory_off",
  "custom_off",
]);

export function InventoryPick({
  account,
  open,
  customItems,
  onClose,
  onPicked,
}: {
  account: CloudAccount;
  open: boolean;
  customItems: readonly CustomLike[];
  onClose: () => void;
  onPicked: (picked: PickedItem) => void;
}) {
  const { t } = useTranslation();
  const animate = useInteractionMotion();
  return (
    <RadixDialog.Root open={open} onOpenChange={(next) => !next && onClose()}>
      <AnimatePresence>
        {open ? (
          <EntranceGate key="afk-plus-pick">
            <RadixDialog.Portal forceMount>
              <RadixDialog.Overlay asChild forceMount>
                <GatedMotion.div
                  className="fixed inset-0 z-[60] bg-obsidian-950/65"
                  {...scrimMotion(!animate)}
                />
              </RadixDialog.Overlay>
              <div className="pointer-events-none fixed inset-0 z-[60] flex items-center justify-center p-5">
                <RadixDialog.Content
                  asChild
                  forceMount
                  onOpenAutoFocus={(event) => {
                    event.preventDefault();
                    focusPanel(event.currentTarget as HTMLElement);
                  }}
                >
                  <GatedMotion.div
                    tabIndex={-1}
                    className="pointer-events-auto flex max-h-[92vh] w-full max-w-[600px] flex-col overflow-hidden rounded-[14px] border border-white/8 bg-obsidian-900 shadow-[0_40px_120px_rgb(34_6_6/0.6)] outline-none"
                    {...dialogMotion(!animate)}
                  >
                    <header className="flex items-start justify-between gap-4 px-5 pt-4 pb-2">
                      <div className="min-w-0">
                        <RadixDialog.Title className="font-display text-section font-semibold text-bone-50">
                          {t("afkPlus.pick.title")}
                        </RadixDialog.Title>
                        <RadixDialog.Description className="mt-1 text-[13px] text-ash-300">
                          {t("afkPlus.pick.exact")}
                        </RadixDialog.Description>
                      </div>
                      <RadixDialog.Close asChild>
                        <IconButton label={t("common.close")} icon={<X size={18} />} size="sm" />
                      </RadixDialog.Close>
                    </header>
                    <PickBody
                      account={account}
                      customItems={customItems}
                      onClose={onClose}
                      onPicked={onPicked}
                    />
                  </GatedMotion.div>
                </RadixDialog.Content>
              </div>
            </RadixDialog.Portal>
          </EntranceGate>
        ) : null}
      </AnimatePresence>
    </RadixDialog.Root>
  );
}

/** Mounted only while the dialog is open: the inventory streams only then. */
function PickBody({
  account,
  customItems,
  onClose,
  onPicked,
}: {
  account: CloudAccount;
  customItems: readonly CustomLike[];
  onClose: () => void;
  onPicked: (picked: PickedItem) => void;
}) {
  const { t } = useTranslation();
  const lang = currentLang();
  const live = useCloudInventory(account, isTauri());
  // A Projects menu's title pictures, on its servers (IA-ART), as the account panel draws them.
  const titleGlyph = usePlusTitleGlyph();
  const preview = isTauri() ? null : mockPickInventory(account.id);
  const state = preview ?? live.state;
  const mode = preview ? "view" : live.mode;
  // A click chooses (its highlight stays); "Bunu seç" asks the machine what it is.
  const [chosen, setChosen] = useState<number | null>(null);
  const [asking, setAsking] = useState(false);
  const [line, setLine] = useState<string | null>(null);
  // The slot the machine just turned down ("Sandıklar … seçilemez"): asking again waits for another.
  const [refused, setRefused] = useState<number | null>(null);
  const inGame = account.live.state === "running";
  const item =
    chosen === null ? undefined : state.inventory?.items.find((it) => it.slot === chosen);

  const reason =
    mode === "off"
      ? "service"
      : state.status === "off"
        ? (state.reason ?? "service")
        : !inGame && state.status !== "ok"
          ? "not_running"
          : null;

  const confirm = async () => {
    if (asking || !item) return;
    setAsking(true);
    setLine(null);
    try {
      const outcome = await plusApi.pick(account.id, {
        slot: item.slot,
        item: item.id,
        cmd: item.cmd ?? null,
        model: item.model ?? null,
      });
      switch (outcome.kind) {
        case "picked": {
          const picked = pickedOf(outcome.pick, customItems);
          if (picked) {
            onPicked(picked);
            return;
          }
          setLine(t("afkPlus.pick.failed.unparsed"));
          break;
        }
        case "failed":
          setLine(
            t(`afkPlus.pick.failed.${FAILED.has(outcome.reason) ? outcome.reason : "unparsed"}`),
          );
          setRefused(item.slot);
          break;
        case "offline":
          setLine(t("afkPlus.pick.offline"));
          break;
        case "limited":
          setLine(t("afkPlus.pick.limited"));
          break;
      }
    } catch (error) {
      afkFailed(error, t("afkPlus.pick.error"));
    }
    setAsking(false);
  };

  if (reason) {
    return (
      <>
        <div className="flex flex-col items-center gap-3 px-5 pt-8 pb-10 text-center">
          <ContentIcon kind="inventory" size={32} />
          <p className="max-w-[40ch] text-[13.5px] text-bone-100">
            {t(`afkPlus.pick.off.${reasonKey(reason)}`)}
          </p>
        </div>
        <footer className="flex justify-end border-t border-white/7 bg-obsidian-950/30 px-5 py-3">
          {/* Nothing to pick here: the dialog only closes. */}
          <Button variant="secondary" onClick={onClose}>
            {t("common.close")}
          </Button>
        </footer>
      </>
    );
  }

  const loading = state.status === "waiting" && !state.inventory;
  const empty = Boolean(state.inventory) && state.inventory?.items.length === 0;
  const name = item ? item.name?.trim() || item.label[lang] || item.id : null;
  const status = line
    ? line
    : item && name
      ? t("afkPlus.pick.chosen", { item: goodsText(item.count, name, lang) })
      : empty
        ? t("afkPlus.pick.empty")
        : "";
  return (
    <>
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto px-5 pt-2 pb-4">
        {/* While it loads, the faded window says so over itself: an empty grid alone read as
            "my items are gone" (FEAT-WEB). */}
        <div className="relative w-full">
          <InventoryView
            inventory={state.inventory}
            window={state.window}
            mode="pick"
            picked={chosen}
            onPick={(slot) => {
              setChosen(slot);
              setLine(null);
            }}
            lang={lang}
            maxScale={3}
            allowDrop={false}
            titleGlyph={titleGlyph}
            // A container query sizes the art: the view needs the dialog's width, not its own.
            className={cn("w-full transition-opacity duration-200", loading && "opacity-40")}
          />
          <AnimatePresence initial={false}>
            {loading ? (
              <motion.p
                key="loading"
                role="status"
                className="pointer-events-none absolute inset-0 grid place-items-center text-[13px] font-medium text-bone-50 [text-shadow:0_1px_2px_rgb(0_0_0/0.6)]"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: enter(0.18) }}
                exit={{ opacity: 0, transition: leave(0.12) }}
              >
                {t("afkPlus.pick.loading")}
              </motion.p>
            ) : null}
          </AnimatePresence>
        </div>
      </div>
      <footer className="flex items-center justify-between gap-3 border-t border-white/7 bg-obsidian-950/30 px-5 py-3">
        <p
          role={line ? "alert" : "status"}
          className={cn(
            "min-w-0 flex-1 truncate text-[12.5px]",
            line ? "text-flare-300" : "text-ash-300",
          )}
        >
          {status}
        </p>
        <Button variant="ghost" onClick={onClose}>
          {t("afkPlus.pick.cancel")}
        </Button>
        <Button
          variant="primary"
          disabled={!item || asking || item.slot === refused}
          onClick={() => void confirm()}
        >
          {t("afkPlus.pick.confirm")}
        </Button>
      </footer>
    </>
  );
}

/** The machine's reasons as the dialog says them. */
function reasonKey(reason: string): "notRunning" | "setting" | "restart" | "version" | "service" {
  switch (reason) {
    case "not_running":
      return "notRunning";
    case "setting":
      return "setting";
    case "restart":
      return "restart";
    case "version":
      return "version";
    default:
      return "service";
  }
}
