// @vitest-environment happy-dom
// INV-UI: the cloud inventory in a document (packages/ui/src/components/inventory): what the
// mouse, the keys and a finger send, what a slot shows while a move waits, a refusal, "too fast",
// and the tooltip and action menu. The machine is a hand-driven stand-in: each move waits until
// the test answers it, and frames come when the test renders them.
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  SAMPLE_CHEST,
  SAMPLE_DISPENSER,
  SAMPLE_INVENTORY,
  SAMPLE_KITS,
  SAMPLE_MARKET,
  withCursor,
} from "../../../../packages/ui/src/components/inventory/fixtures";
import {
  InventoryView,
  type InventoryViewProps,
} from "../../../../packages/ui/src/components/inventory/inventory-view";
import type {
  InvFrame,
  InvOp,
  InvResult,
  TitleGlyph,
  WinFrame,
} from "../../../../packages/ui/src/components/inventory/types";

interface Sent {
  op: InvOp;
  answer: (result: InvResult) => Promise<void>;
}

function machine() {
  const sent: Sent[] = [];
  const onOp = vi.fn(
    (op: InvOp) =>
      new Promise<InvResult>((resolve) => {
        sent.push({
          op,
          answer: (result) =>
            act(async () => {
              resolve(result);
            }),
        });
      }),
  );
  return { sent, onOp };
}

function mount(props: Partial<InventoryViewProps> = {}) {
  const m = machine();
  const base: InventoryViewProps = { inventory: SAMPLE_INVENTORY, onOp: m.onOp, ...props };
  const view = render(<InventoryView {...base} />);
  /** A new frame from the machine, as the live socket would bring it. */
  const frame = (inventory: InvFrame, window: WinFrame | null = base.window ?? null) =>
    view.rerender(<InventoryView {...base} inventory={inventory} window={window} />);
  return { ...m, ...view, frame };
}

const cell = (key: string) => {
  const el = document.querySelector<HTMLElement>(`[data-slot="${key}"]`);
  if (!el) throw new Error(`no slot ${key}`);
  return el;
};
const has = (el: Element | null | undefined, attribute: string) =>
  Boolean(el?.hasAttribute(attribute));
const label = (key: string) => cell(key).getAttribute("aria-label");
const mouse = { pointerType: "mouse", pointerId: 1, clientX: 40, clientY: 40 };
const finger = { pointerType: "touch", pointerId: 7, clientX: 60, clientY: 60 };
const status = () => screen.getByRole("status").textContent ?? "";
const spoken = () => document.querySelector("[aria-live=polite]")?.textContent ?? "";
const tooltip = () => document.querySelector<HTMLElement>(".pl-inv-tip");
const carried = () => document.querySelector(".pl-inv-cursor");
const flashed = () => document.querySelector(".pl-inv-flash") !== null;
/** The words the game's pixels spell inside an element (each drawn line carries its text). */
const words = (el: Element | null | undefined) =>
  [...(el?.querySelectorAll(".pl-inv-tip-line") ?? [])].map((l) => l.getAttribute("data-text"));
const rows = (menu: HTMLElement) =>
  within(menu)
    .getAllByRole("menuitem")
    .map((i) => [
      i.querySelector("span")?.textContent,
      i.querySelector("kbd")?.textContent ?? null,
    ]);

const pickup = (area: "inv" | "win", slot: number, wseq = 0, button: 0 | 1 = 0) => ({
  action: "click",
  area,
  slot,
  button,
  mode: "pickup",
  wseq,
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("the mouse, as in the game", () => {
  it("takes a stack with a left click: sent at once, the slot waits, the frame settles it", async () => {
    const { sent, frame } = mount();
    fireEvent.pointerDown(cell("inv:13"), { ...mouse, button: 0 });
    expect(sent.map((s) => s.op)).toEqual([pickup("inv", 13)]);
    // The player's own screen shows the move at once and marks it waiting.
    expect(has(cell("inv:13"), "data-pending")).toBe(true);
    expect(cell("inv:13").getAttribute("aria-busy")).toBe("true");
    expect(label("inv:13")).toBe("Boş, bekliyor");
    expect(carried()).not.toBeNull();
    await sent[0]?.answer({ ok: true });
    expect(has(cell("inv:13"), "data-pending")).toBe(true);
    act(() => frame(withCursor(SAMPLE_INVENTORY, 13)));
    expect(has(cell("inv:13"), "data-pending")).toBe(false);
    expect(spoken()).toBe("Elmas alındı");
  });

  it("takes half with a right click and never opens a menu", () => {
    const { sent } = mount();
    fireEvent.pointerDown(cell("inv:14"), { ...mouse, button: 2 });
    expect(sent.map((s) => s.op)).toEqual([pickup("inv", 14, 0, 1)]);
    expect(fireEvent.contextMenu(cell("inv:14"))).toBe(false);
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("moves with Shift+click and ignores the second press of a double click", () => {
    const { sent } = mount();
    fireEvent.pointerDown(cell("inv:9"), { ...mouse, button: 0, shiftKey: true });
    expect(sent[0]?.op).toMatchObject({ slot: 9, mode: "quick_move" });
    fireEvent.pointerDown(cell("inv:11"), { ...mouse, button: 0 });
    fireEvent.pointerDown(cell("inv:11"), { ...mouse, button: 0 });
    expect(sent).toHaveLength(2);
  });

  it("shows the game's tooltip on hover and drops with Q over the slot", () => {
    const { sent } = mount();
    fireEvent.pointerOver(cell("inv:36"), mouse);
    const tip = tooltip();
    expect(has(tip, "data-shown")).toBe(true);
    expect(words(tip)).toEqual([
      "Elmas Kılıç",
      "Keskinlik V",
      "Kırılmazlık III",
      "Onarım",
      "Dayanıklılık: 1440 / 1561",
    ]);
    // Drawn as pixels: one picture a line, its shadow under its letters.
    const name = tip?.querySelector("svg.pl-inv-line");
    expect(name?.querySelectorAll("path")).toHaveLength(2);
    expect(name?.querySelector("path")?.getAttribute("fill")).toBe("#153f3f");
    // The torches (64): one, then the rest of the stack.
    fireEvent.pointerOver(cell("inv:41"), mouse);
    fireEvent.keyDown(document.body, { key: "q" });
    fireEvent.keyDown(document.body, { key: "q", ctrlKey: true });
    expect(sent.map((s) => s.op)).toEqual([
      { action: "drop", area: "inv", slot: 41, stack: false, wseq: 0 },
      { action: "drop", area: "inv", slot: 41, stack: true, wseq: 0 },
    ]);
  });

  it("puts a carried item back on a press outside the window, never drops it", () => {
    const { sent } = mount({ inventory: withCursor(SAMPLE_INVENTORY, 13) });
    fireEvent.pointerDown(document.body, { ...mouse, button: 0 });
    // The first free slot of the main inventory (this view did not pick it up itself).
    expect(sent.map((s) => s.op)).toEqual([pickup("inv", 13)]);
  });
});

describe("the keyboard", () => {
  it("is one tab stop the arrows move across the grids", () => {
    mount();
    const start = cell("inv:36");
    expect(start.getAttribute("tabindex")).toBe("0");
    act(() => start.focus());
    fireEvent.keyDown(start, { key: "ArrowRight" });
    expect(document.activeElement).toBe(cell("inv:37"));
    expect(cell("inv:37").getAttribute("tabindex")).toBe("0");
    expect(start.getAttribute("tabindex")).toBe("-1");
    fireEvent.keyDown(cell("inv:37"), { key: "ArrowUp" });
    expect(document.activeElement).toBe(cell("inv:28"));
    fireEvent.keyDown(cell("inv:28"), { key: "End", ctrlKey: true });
    expect(document.activeElement).toBe(cell("inv:44"));
    expect(screen.getAllByRole("grid").map((g) => g.getAttribute("aria-label"))).toEqual([
      "Ekipman",
      "Envanter",
      "Araç çubuğu",
    ]);
    expect(label("inv:36")).toBe("Elmas Kılıç, dayanıklılık yüzde 92, büyülü, elde");
  });

  it("names the player's own screen and draws counts as the game's pixels", () => {
    mount();
    const titles = [...document.querySelectorAll(".pl-inv-title")].map((t) =>
      t.getAttribute("data-text"),
    );
    expect(titles).toEqual(["Ekipman", "Envanter"]);
    const count = cell("inv:13").querySelector(".pl-inv-count");
    expect(count?.tagName.toLowerCase()).toBe("svg");
    expect(count?.getAttribute("data-text")).toBe("9");
    expect(cell("inv:36").querySelector(".pl-inv-count")).toBeNull();
    // The held slot is framed like the game's hotbar marks it.
    const frame = document.querySelector<HTMLElement>(".pl-inv-held");
    expect(frame?.style.translate).toBe("calc(5 * var(--pl-inv-px)) calc(106 * var(--pl-inv-px))");
  });

  it("drops with Q and Ctrl+Q, moves with Shift+Enter, clicks with Enter", () => {
    const { sent } = mount();
    fireEvent.keyDown(cell("inv:10"), { key: "q" });
    fireEvent.keyDown(cell("inv:10"), { key: "Q", ctrlKey: true });
    fireEvent.keyDown(cell("inv:9"), { key: "Enter", shiftKey: true });
    fireEvent.keyDown(cell("inv:13"), { key: "Enter" });
    expect(sent.map((s) => s.op)).toEqual([
      { action: "drop", area: "inv", slot: 10, stack: false, wseq: 0 },
      { action: "drop", area: "inv", slot: 10, stack: true, wseq: 0 },
      { action: "click", area: "inv", slot: 9, button: 0, mode: "quick_move", wseq: 0 },
      pickup("inv", 13),
    ]);
    // With an item on the cursor nothing drops (the machine would refuse it).
    fireEvent.keyDown(cell("inv:11"), { key: "q" });
    expect(sent).toHaveLength(4);
  });

  it("trades places with the hotbar on a number key, one click after another", async () => {
    const { sent, frame } = mount();
    // Diamonds (13) and the sword (36): three clicks, each sent once the frame shows its cursor.
    fireEvent.keyDown(cell("inv:13"), { key: "1" });
    expect(sent.map((s) => s.op)).toEqual([pickup("inv", 13)]);
    await sent[0]?.answer({ ok: true });
    act(() => frame(withCursor(SAMPLE_INVENTORY, 13)));
    expect(sent.map((s) => s.op)).toEqual([pickup("inv", 13), pickup("inv", 36)]);
  });

  it("puts the cursor's item back with Esc, then closes the menu with Esc", async () => {
    const { sent, frame } = mount({ window: SAMPLE_CHEST });
    const chest = SAMPLE_CHEST.wseq;
    fireEvent.pointerDown(cell("win:0"), { ...mouse, button: 0 });
    expect(sent[0]?.op).toEqual(pickup("win", 0, chest));
    // A window's slot waits for the frame and keeps showing its item meanwhile.
    expect(label("win:0")).toBe("Demir Külçesi, 64 adet, bekliyor");
    expect(carried()).toBeNull();
    await sent[0]?.answer({ ok: true });
    const [iron, ...rest] = SAMPLE_CHEST.items;
    if (!iron) throw new Error("fixture");
    const { slot: _slot, ...ingots } = iron;
    act(() => frame({ ...SAMPLE_INVENTORY, cursor: ingots }, { ...SAMPLE_CHEST, items: rest }));
    expect(carried()).not.toBeNull();
    // Back where it came from.
    fireEvent.keyDown(cell("inv:9"), { key: "Escape" });
    expect(sent[1]?.op).toEqual(pickup("win", 0, chest));
    await sent[1]?.answer({ ok: true });
    act(() => frame(SAMPLE_INVENTORY, SAMPLE_CHEST));
    fireEvent.keyDown(cell("inv:9"), { key: "Escape" });
    expect(sent[2]?.op).toEqual({ action: "close", wseq: chest });
  });

  it("opens the slot's actions with Shift+F10, headed by the item's tooltip, with their keys", () => {
    const { sent } = mount();
    act(() => cell("inv:38").focus());
    fireEvent.keyDown(cell("inv:38"), { key: "F10", shiftKey: true });
    const menu = screen.getByRole("menu", { name: "Altın Elma" });
    expect(words(menu)).toEqual(["Altın Elma"]);
    expect(rows(menu)).toEqual([
      ["Al", "Enter"],
      ["Yarısını al", null],
      ["Envantere taşı", "Shift+Enter"],
      ["Elde tut", null],
      ["Kullan", null],
      ["Bir tane at", "Q"],
      ["Yığını at", "Ctrl+Q"],
    ]);
    // A solid surface over the game's window.
    expect(menu.classList.contains("pl-inv-menu")).toBe(true);
    // The floating tooltip makes way for the menu's own.
    expect(has(tooltip(), "data-shown")).toBe(false);
    fireEvent.click(within(menu).getByRole("menuitem", { name: "Elde tut" }));
    expect(sent.map((s) => s.op)).toEqual([{ action: "hold", slot: 2 }]);
    // The held frame moves at once (the account's hand is the player's own screen).
    const frame = document.querySelector<HTMLElement>(".pl-inv-held");
    expect(frame?.style.translate).toContain("calc(41 *");
  });

  it("names the clicks themselves in a server's window, and drops only from the player's slots", () => {
    const { sent } = mount({ window: SAMPLE_KITS });
    act(() => cell("win:13").focus());
    fireEvent.keyDown(cell("win:13"), { key: "F10", shiftKey: true });
    const menu = screen.getByRole("menu", { name: "Savaşçı Kiti" });
    // The server decides what they do (a kit may be bought): no guess at a verb.
    expect(rows(menu)).toEqual([
      ["Sol tık", "Enter"],
      ["Sağ tık", null],
      ["Shift+sol tık", "Shift+Enter"],
    ]);
    // The window's close button stays in place, unavailable, while the slot's menu is open.
    const close = screen.getByRole("button", {
      name: "Menüyü kapat",
      hidden: true,
    }) as HTMLButtonElement;
    expect(close.disabled).toBe(true);
    fireEvent.click(within(menu).getByText("Sol tık"));
    expect(sent[0]?.op).toEqual(pickup("win", 13, SAMPLE_KITS.wseq));
  });
});

describe("a finger", () => {
  it("takes with a tap", () => {
    const { sent } = mount();
    fireEvent.pointerDown(cell("inv:13"), finger);
    fireEvent.pointerUp(cell("inv:13"), finger);
    expect(sent.map((s) => s.op)).toEqual([pickup("inv", 13)]);
  });

  it("opens the slot's actions with a long press, and scrolling opens nothing", () => {
    vi.useFakeTimers();
    const { sent } = mount();
    fireEvent.pointerDown(cell("inv:22"), finger);
    fireEvent.pointerMove(cell("inv:22"), { ...finger, clientY: 90 });
    act(() => vi.advanceTimersByTime(600));
    expect(screen.queryByRole("menu")).toBeNull();
    fireEvent.pointerDown(cell("inv:22"), finger);
    act(() => vi.advanceTimersByTime(600));
    fireEvent.pointerUp(cell("inv:22"), finger);
    const menu = screen.getByRole("menu", { name: "Netherit Kazma" });
    expect(words(menu)).toContain("Verimlilik V");
    expect(sent).toHaveLength(0);
    // Under a finger the rows carry no keys; one item drops as "At".
    expect(rows(menu)).toEqual([
      ["Al", null],
      ["Araç çubuğuna taşı", null],
      ["At", null],
    ]);
    fireEvent.click(within(menu).getByRole("menuitem", { name: "Araç çubuğuna taşı" }));
    expect(sent[0]?.op).toMatchObject({ slot: 22, mode: "quick_move" });
  });

  it("keeps a carried stack in a hand slot under the window, and says what a tap does", () => {
    mount();
    fireEvent.pointerDown(cell("inv:13"), finger);
    fireEvent.pointerUp(cell("inv:13"), finger);
    // Nothing floats and the grid shows its slots as they are: the diamonds are in the hand.
    expect(carried()).toBeNull();
    expect(cell("inv:13").querySelector("[data-item]")).toBeNull();
    const hand = document.querySelector(".pl-inv-hand");
    expect(hand?.querySelector("[data-item]")?.getAttribute("data-item")).toBe("minecraft:diamond");
    expect(hand?.querySelector(".pl-inv-count")?.getAttribute("data-text")).toBe("9");
    expect(status()).toBe("Bırakmak için bir yuvaya dokun.");
    // The way back in words, beside the hint.
    expect(screen.getByRole("button", { name: "Yerine koy" })).toBeTruthy();
  });

  it("puts a carried stack back with a tap on the hand slot", async () => {
    const { sent, frame } = mount();
    fireEvent.pointerDown(cell("inv:13"), finger);
    fireEvent.pointerUp(cell("inv:13"), finger);
    await sent[0]?.answer({ ok: true });
    act(() => frame(withCursor(SAMPLE_INVENTORY, 13)));
    fireEvent.click(screen.getByRole("button", { name: "Yerine koy: Elmas" }));
    // Back where it came from, as Esc and a press outside the window do.
    expect(sent.map((s) => s.op)).toEqual([pickup("inv", 13), pickup("inv", 13)]);
  });

  it("shows the tooltip with a tap where nothing moves", () => {
    const { onOp } = mount({ window: SAMPLE_CHEST, mode: "view" });
    // Looking only says so all along; a tap is the tooltip and nothing else.
    expect(status()).toBe("Bu envanterde değişiklik yapamazsın.");
    fireEvent.pointerDown(cell("win:13"), finger);
    fireEvent.pointerUp(cell("win:13"), finger);
    expect(has(tooltip(), "data-shown")).toBe(true);
    expect(words(tooltip())[0]).toBe("Demir Kazma");
    expect(flashed()).toBe(false);
    expect(onOp).not.toHaveBeenCalled();
    fireEvent.pointerDown(document.body, finger);
    expect(has(tooltip(), "data-shown")).toBe(false);
    expect(document.querySelector(".pl-inv-lock")).toBeNull();
    // A click tries to move: the line brightens once and a lock shows where it was tried.
    fireEvent.pointerDown(cell("win:13"), { ...mouse, button: 0 });
    expect(flashed()).toBe(true);
    expect(cell("win:13").querySelector(".pl-inv-lock")).not.toBeNull();
    expect(document.querySelectorAll(".pl-inv-lock")).toHaveLength(1);
    // The item stays as it is.
    expect(cell("win:13").querySelector("[data-item]")).not.toBeNull();
    expect(onOp).not.toHaveBeenCalled();
  });
});

describe("answers", () => {
  beforeEach(() => vi.useFakeTimers());

  it("rolls a refused move back and says why in one line", async () => {
    const { sent } = mount();
    fireEvent.pointerDown(cell("inv:13"), { ...mouse, button: 0 });
    await sent[0]?.answer({ error: { code: "busy_trade" } });
    expect(has(cell("inv:13"), "data-pending")).toBe(false);
    expect(label("inv:13")).toBe("Elmas, 9 adet");
    expect(carried()).toBeNull();
    expect(status()).toBe("Takas sürerken envanter kilitli.");
    act(() => vi.advanceTimersByTime(3000));
    expect(status()).toBe("");
  });

  it("says the server refused when no frame follows an ok", async () => {
    const { sent } = mount();
    fireEvent.pointerDown(cell("inv:13"), { ...mouse, button: 0 });
    await sent[0]?.answer({ ok: true });
    act(() => vi.advanceTimersByTime(1700));
    expect(status()).toBe("Sunucu bu hamleye izin vermedi.");
    expect(label("inv:13")).toBe("Elmas, 9 adet");
  });

  it("redraws a stale move quietly", async () => {
    const { sent } = mount();
    fireEvent.pointerDown(cell("inv:13"), { ...mouse, button: 0 });
    await sent[0]?.answer({ error: { code: "stale" } });
    expect(status()).toBe("");
    expect(label("inv:13")).toBe("Elmas, 9 adet");
  });

  it("waits out 'too fast' calmly and never sends again by itself", async () => {
    const { sent } = mount();
    fireEvent.pointerDown(cell("inv:13"), { ...mouse, button: 0 });
    await sent[0]?.answer({ error: { code: "rate_limited", retryAfter: 2 } });
    expect(status()).toBe("Biraz yavaşla; son tıklama sayılmadı.");
    fireEvent.pointerDown(cell("inv:14"), { ...mouse, button: 0 });
    expect(sent).toHaveLength(1);
    expect(status()).toBe("Biraz yavaşla; son tıklama sayılmadı.");
    act(() => vi.advanceTimersByTime(10_000));
    expect(sent).toHaveLength(1);
    fireEvent.pointerDown(cell("inv:14"), { ...mouse, button: 0 });
    expect(sent).toHaveLength(2);
  });

  it("keeps everything still in a trade, the close button too", () => {
    const { onOp } = mount({ window: SAMPLE_CHEST, locked: true });
    expect(status()).toBe("Takas sürerken envanter kilitli.");
    const close = screen.getByRole("button", { name: "Menüyü kapat" }) as HTMLButtonElement;
    expect(close.disabled).toBe(true);
    expect(flashed()).toBe(false);
    fireEvent.pointerDown(cell("win:0"), { ...mouse, button: 0 });
    expect(onOp).not.toHaveBeenCalled();
    // The try brightens the lock's line (outside the live region: it is said once) and shows a
    // lock on the slot.
    expect(flashed()).toBe(true);
    expect(cell("win:0").querySelector(".pl-inv-lock")).not.toBeNull();
    expect(status()).toBe("Takas sürerken envanter kilitli.");
  });
});

describe("a picture", () => {
  it("takes no input and offers no stops in static mode", () => {
    const { onOp } = mount({ mode: "static", label: "Örnek envanter" });
    expect(document.querySelectorAll("[tabindex]")).toHaveLength(0);
    expect(screen.queryByRole("status")).toBeNull();
    expect(screen.getByRole("group", { name: "Örnek envanter" })).toBeTruthy();
    fireEvent.pointerDown(cell("inv:13"), { ...mouse, button: 0 });
    expect(onOp).not.toHaveBeenCalled();
  });
});

describe("window titles", () => {
  const titles = () =>
    [...document.querySelectorAll<HTMLElement>(".pl-inv-title")].map((t) =>
      t.getAttribute("data-text"),
    );

  it("draws a Projects menu with its own picture under the slots, as the game lays it out", () => {
    const glyphs = new Map<number, TitleGlyph>([
      [0xf808, { advance: -8 }],
      [0xe001, { advance: 177, image: { src: "market.png", width: 176, height: 72, ascent: 13 } }],
    ]);
    mount({ window: SAMPLE_MARKET, titleGlyph: (codePoint) => glyphs.get(codePoint) });
    const picture = document.querySelector<HTMLElement>(".pl-inv-glyph");
    expect(picture?.style.backgroundImage).toContain("market.png");
    expect(picture?.style.left).toBe("calc(0 * var(--pl-inv-px))");
    expect(picture?.style.top).toBe("calc(0 * var(--pl-inv-px))");
    // The picture carries the menu's name: no words over it, only the inventory's label.
    expect(titles()).toEqual(["Envanter"]);
    // Earlier in the document than the slots: drawn under them.
    expect(picture?.compareDocumentPosition(cell("win:10"))).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
  });

  it("leaves the glyphs out where the art does not apply, and names the window", () => {
    mount({ window: SAMPLE_MARKET });
    expect(document.querySelector(".pl-inv-glyph")).toBeNull();
    expect(titles()).toEqual(["Menü", "Envanter"]);
  });

  it("centres a 3 x 3 window's title on its width, as the game does", () => {
    mount({ window: SAMPLE_DISPENSER });
    const name = document.querySelector<HTMLElement>(".pl-inv-title");
    expect(name?.getAttribute("data-text")).toBe("Fırlatıcı");
    // (176 - 37) / 2, rounded down.
    expect(name?.style.left).toBe("calc(69 * var(--pl-inv-px))");
  });
});
