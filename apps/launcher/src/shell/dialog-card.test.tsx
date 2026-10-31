// @vitest-environment happy-dom
// DIALOG-CARD: a server's dialog checked as the contract writes it, the command its inputs build
// as the client builds it, and the card that never runs a command the player has not seen.
// CONSOLE-ANY: a client 4.2.5 dialog as a real form, its values written as the game writes them.
import {
  type Dialog,
  DialogCard,
  DialogMenus,
  type DialogRangeInput,
  DialogWaiting,
  dialogCommand,
  dialogRefusal,
  escapeText,
  fieldText,
  initialValues,
  javaFloat,
  lineRuns,
  numberText,
  parseTemplate,
  safeDialog,
  safeMenus,
  sliderLabel,
  sliderTakes,
  sliderText,
  sliderValue,
} from "@pl/ui/console";
import { TooltipProvider } from "@pl/ui";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactElement } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

const show = (ui: ReactElement) => render(<TooltipProvider>{ui}</TooltipProvider>);

const RAW = {
  title: "Takas",
  titleSt: [[5, "6l"]],
  body: [{ text: "MorKedi_07 ile takas\nemin misin?" }],
  inputs: [
    { key: "amount", kind: "range", label: "Adet", start: 1, end: 64, step: 1, initial: 16 },
    { key: "note", kind: "text", label: "Not", max: 32, initial: "" },
    { key: "fast", kind: "bool", label: "Hızlı", initial: false, onTrue: "evet", onFalse: "hayir" },
    {
      key: "item",
      kind: "option",
      label: "Eşya",
      options: [
        { id: "diamond", label: "Elmas" },
        { id: "emerald", label: "Zümrüt" },
      ],
      initial: "diamond",
    },
  ],
  buttons: [
    { i: 1, label: "Gönder", a: "run_command", v: "/trade send $(item) $(amount) $(fast) $(note)" },
    { i: 2, label: "Kurallar", a: "open_url", v: "https://projects.gg/kurallar" },
    { i: 3, label: "Kabul", a: "custom" },
  ],
  cancel: true,
  play: true,
  cols: 2,
};

/** The same dialog from a client 4.2.5: its command a template the game fills in. */
const EXACT = {
  ...RAW,
  buttons: [{ ...RAW.buttons[0], tpl: true }, RAW.buttons[1], RAW.buttons[2]],
};

describe("a server's dialog", () => {
  it("reads only as the contract writes it", () => {
    const dialog = safeDialog(RAW) as Dialog;
    expect(dialog).not.toBeNull();
    expect(dialog.inputs.map((input) => input.kind)).toEqual(["range", "text", "bool", "option"]);
    expect(dialog.buttons.map((button) => button.a)).toEqual(["run_command", "open_url", "custom"]);
    expect(dialog.after).toBe("close");
    const odd = (patch: Record<string, unknown>) => safeDialog({ ...RAW, ...patch });
    expect(odd({ title: "a\u202eb" })).toBeNull();
    expect(odd({ title: "x".repeat(129) })).toBeNull();
    expect(odd({ buttons: [{ i: 1, label: "x", a: "run_command", v: "trade" }] })).toBeNull();
    expect(
      odd({ buttons: [{ i: 1, label: "x", a: "open_url", v: "http://projects.gg" }] }),
    ).toBeNull();
    expect(odd({ buttons: [{ i: 1, label: "x", a: "open_file", v: "C:/x" }] })).toBeNull();
    // Up to 64 buttons (client 4.2.5).
    const many = (n: number) => Array.from({ length: n }, (_, i) => ({ i, label: "x", a: "close" }));
    expect(odd({ buttons: many(64) })).not.toBeNull();
    expect(odd({ buttons: many(65) })).toBeNull();
    expect(odd({ inputs: [{ key: "Bad Key", kind: "text", label: "x", max: 8 }] })).toBeNull();
    expect(odd({ inputs: [{ key: "n", kind: "text", label: "x", max: 999 }] })).toBeNull();
    expect(odd({ body: [{ text: "y".repeat(20_000) }] })).toBeNull();
    // A part's looks that do not cover it go; the part stays.
    expect(safeDialog({ ...RAW, titleSt: [[2, "6"]] })?.titleSt).toBeNull();
    const zw = String.fromCodePoint(0x200b);
    expect(odd({ title: `Ta${zw}kas` })).toBeNull();
    // An item part needs its labels (as the hovers carry items) and its text.
    expect(odd({ body: [{ item: { id: "minecraft:dirt" } }] })).toBeNull();
    expect(odd({ body: [{ text: "", item: { id: "minecraft:dirt" } }] })).toBeNull();
    expect(
      odd({ buttons: [{ i: 1, label: "x", a: "open_url", v: "https://xn--prjects-6ve.gg/" }] }),
    ).toBeNull();
    const multiline = safeDialog({
      ...RAW,
      inputs: [
        { key: "Note_2", kind: "text", label: "Not", max: 64, multiline: true, initial: "a\nb" },
      ],
    });
    expect(multiline?.inputs[0]).toMatchObject({ key: "Note_2", multiline: true, initial: "a\nb" });
    expect(
      odd({ inputs: [{ key: "n", kind: "text", label: "x", max: 32, initial: "a\nb" }] }),
    ).toBeNull();
  });

  it("reads client 4.2.5's details", () => {
    const dialog = safeDialog({
      ...EXACT,
      body: [
        {
          text: "Ödül",
          item: { id: "minecraft:diamond", count: 2, label: { tr: "Elmas", en: "Diamond" } },
          noTip: true,
        },
      ],
      inputs: [
        { ...RAW.inputs[0], fmt: "%s: %s" },
        { key: "note", kind: "text", label: "Not", max: 64, multiline: true, lines: 4, initial: "" },
      ],
      buttons: [
        EXACT.buttons[0],
        { ...RAW.buttons[2], tip: "Kuralları kabul eder", tipSt: [[20, "e"]], w: 120 },
      ],
      after: "wait",
      exit: { label: "Vazgeç" },
      cut: true,
    }) as Dialog;
    expect(dialog).not.toBeNull();
    expect(dialog.body[0]?.item?.id).toBe("minecraft:diamond");
    expect(dialog.body[0]?.noTip).toBe(true);
    expect(dialog.inputs[1]).toMatchObject({ lines: 4, multiline: true });
    expect(dialog.buttons[0]?.tpl).toBe(true);
    expect(dialog.buttons[1]).toMatchObject({ tip: { text: "Kuralları kabul eder" }, w: 120 });
    expect(dialog.after).toBe("wait");
    expect(dialog.exit?.text).toBe("Vazgeç");
    expect(dialog.cut).toBe(true);
    // A template the game would refuse never shows; tpl only on a command.
    const odd = (button: Record<string, unknown>) => safeDialog({ ...RAW, buttons: [button] });
    expect(odd({ i: 1, label: "x", a: "run_command", v: "/spawn", tpl: true })).toBeNull();
    expect(odd({ i: 1, label: "x", a: "run_command", v: "/spawn", tpl: false })).not.toBeNull();
    expect(odd({ i: 1, label: "x", a: "custom", tpl: true })).toBeNull();
    expect(safeDialog({ ...RAW, after: "later" })).toBeNull();
  });

  it("builds a 4.2.4 command as 4.2.4 does: in order, escaped text, the switch's words", () => {
    const dialog = safeDialog(RAW) as Dialog;
    const button = dialog.buttons[0];
    if (!button) throw new Error("no button");
    const values = { ...initialValues(dialog), note: 'a "b" \\c', fast: true, amount: 32 };
    expect(dialogCommand(dialog, button, values)).toBe(
      '/trade send diamond 32 evet a \\"b\\" \\\\c',
    );
    // A `$` in a value is never a pattern, and a value never fills a later key.
    expect(dialogCommand(dialog, button, { ...values, note: "$(amount) $&" })).toBe(
      "/trade send diamond 32 evet $(amount) $&",
    );
    expect(dialogCommand(dialog, button, { ...values, item: "netherite" })).toBeNull();
    expect(dialogCommand(dialog, button, { ...values, note: "x".repeat(33) })).toBeNull();
    expect(dialogCommand(dialog, button, { ...values, amount: 65 })).toBeNull();
    expect(dialogCommand(dialog, button, { ...values, note: "a\u0000" })).toBeNull();
    expect(dialogCommand({ ...dialog, play: false }, button, values)).toBeNull();
  });

  it("builds a 4.2.5 command as the game does", () => {
    const dialog = safeDialog(EXACT) as Dialog;
    const button = dialog.buttons[0];
    if (!button) throw new Error("no button");
    const values = { ...initialValues(dialog), note: "don't \"$(amount)\"", fast: true };
    // Each variable once, never inside a value; a text escaped as StringTag.escapeWithoutQuotes.
    expect(dialogCommand(dialog, button, values)).toBe(
      "/trade send diamond 16 evet don\\'t \\\"$(amount)\\\"",
    );
    // A plain command is sent as it is; a name without an input is nothing.
    const plain = { ...button, v: "/warp $(note)", tpl: false };
    expect(dialogCommand(dialog, plain, values)).toBe("/warp $(note)");
    const unknown = { ...button, v: "/warp $(nope)x", tpl: true };
    expect(dialogCommand(dialog, unknown, values)).toBe("/warp x");
    // The game's field takes no section sign: no command with one.
    expect(dialogCommand(dialog, button, { ...values, note: "a\u00a7b" })).toBeNull();
  });

  it("writes values as the game and the client write them", () => {
    expect(escapeText("Ka\"an\\ don't\n\u0001")).toBe("Ka\\\"an\\\\ don\\'t\\n\\x01");
    expect(javaFloat(0.5)).toBe("0.5");
    expect(javaFloat(100)).toBe("100.0");
    expect(javaFloat(0.0001)).toBe("1.0E-4");
    expect(javaFloat(1.5e-5)).toBe("1.5E-5");
    expect(javaFloat(12345678)).toBe("1.2345678E7");
    expect(javaFloat(Math.fround(0.1))).toBe("0.1");
    expect(sliderText(5)).toBe("5");
    expect(sliderText(2.5)).toBe("2.5");
    expect(numberText(16)).toBe("16");
    expect(numberText(Math.fround(0.3))).toBe("0.3");
    expect(numberText(Math.fround(0.00001))).toBe("0.00001");
    expect(parseTemplate("/say $(a) $(b)")?.names).toEqual(["a", "b"]);
    expect(parseTemplate("/spawn")).toBeNull();
    expect(parseTemplate("/say $(a")).toBeNull();
    expect(parseTemplate("/say $(a-b)")).toBeNull();
  });

  it("moves a slider in the game's steps, counted from its initial value", () => {
    const range = {
      key: "k",
      kind: "range",
      label: "Adet",
      start: 0,
      end: 10,
      step: 3,
      initial: 1,
      fmt: null,
    } satisfies DialogRangeInput;
    for (const value of [1, 4, 7, 10]) expect(sliderTakes(value, range)).toBe(true);
    for (const value of [0, 2, 11]) expect(sliderTakes(value, range)).toBe(false);
    expect(sliderValue(0.65, range)).toBe(7);
    expect(sliderValue(0, range)).toBe(1);
    expect(sliderLabel(range, 7)).toBe("Adet: 7");
    expect(sliderLabel({ ...range, fmt: "%2$s × %1$s" }, 7)).toBe("7 × Adet");
  });

  it("keeps in a field only what the game's field takes", () => {
    const input = {
      key: "n",
      kind: "text",
      label: "Not",
      max: 8,
      multiline: true,
      lines: 2,
      height: null,
      initial: "",
    } as const;
    expect(fieldText(input, "a\u00a7b\tc")).toBe("abc");
    expect(fieldText(input, "a\nb\nc")).toBe("a\nbc");
    expect(fieldText(input, "abcdefghij")).toBe("abcdefgh");
  });

  it("says why a press was refused without a detail", () => {
    expect(dialogRefusal("stale", "tr")).toBe("Sunucu penceresi değişti.");
    expect(dialogRefusal("cmd", "tr")).toBe("Bu düğme şu an çalışmadı.");
    expect(dialogRefusal("menu", "tr")).toBe("Bu menü şu an yok.");
    expect(dialogRefusal(undefined, "en")).toBe("This button didn't work just now.");
  });

  it("reads the menus as labels, leaving out what is out of shape", () => {
    const zw = String.fromCodePoint(0x200b);
    expect(
      safeMenus({ pause: { label: "Kurallar", labelSt: [[8, "6"]] }, quick: { label: `a${zw}` } }),
    ).toEqual({ pause: { text: "Kurallar", st: [[8, "6"]] } });
    expect(safeMenus(null)).toEqual({});
  });
});

describe("DialogCard", () => {
  it("shows a refused press under its buttons", () => {
    show(
      <DialogCard
        dialog={safeDialog(RAW) as Dialog}
        lang="tr"
        clock="17:30:00"
        error="Bu düğme şu an çalışmadı."
        onAct={vi.fn()}
      />,
    );
    expect(screen.getByRole("alert").textContent).toBe("Bu düğme şu an çalışmadı.");
  });

  it("says in one line that its commands wait for the game while the client joins", () => {
    const dialog = safeDialog(RAW) as Dialog;
    const { rerender } = show(
      <DialogCard dialog={{ ...dialog, play: false }} lang="tr" clock="17:30:00" onAct={vi.fn()} />,
    );
    expect(screen.getByText("Gönder").closest("button")?.disabled).toBe(true);
    expect(screen.getByText("Komut düğmeleri oyuna girince çalışır.")).not.toBeNull();
    rerender(
      <TooltipProvider>
        <DialogCard dialog={dialog} lang="tr" clock="17:30:00" onAct={vi.fn()} />
      </TooltipProvider>,
    );
    expect(screen.getByText("Gönder").closest("button")?.disabled).toBe(false);
    expect(screen.queryByText("Komut düğmeleri oyuna girince çalışır.")).toBeNull();
  });

  it("asks before a command runs, with the command its inputs build", () => {
    const onAct = vi.fn();
    show(<DialogCard dialog={safeDialog(RAW) as Dialog} lang="tr" clock="17:30:00" onAct={onAct} />);
    fireEvent.click(screen.getByText("Gönder"));
    expect(onAct).not.toHaveBeenCalled();
    expect(screen.getByText("Bu komut çalışsın mı?")).not.toBeNull();
    expect(screen.getByText("/trade send diamond 16 hayir")).not.toBeNull();
    expect(screen.getByText("Sunucu penceresinden, 17:30:00")).not.toBeNull();
    fireEvent.click(screen.getByText("Çalıştır"));
    expect(onAct).toHaveBeenCalledWith({
      i: 1,
      inputs: { amount: "16", note: "", fast: "false", item: "diamond" },
      cmd: "/trade send diamond 16 hayir ",
    });
  });

  it("asks before a link opens and tells the client of every press, as the game's buttons", () => {
    const onAct = vi.fn();
    const onClick = vi.fn();
    show(
      <DialogCard
        dialog={safeDialog(RAW) as Dialog}
        lang="tr"
        clock="17:30:00"
        onAct={onAct}
        onClick={onClick}
      />,
    );
    fireEvent.click(screen.getByText("Kurallar"));
    expect(screen.getByText("projects.gg").className).toContain("font-semibold");
    expect(screen.getByText("projects.gg/kurallar")).not.toBeNull();
    fireEvent.click(screen.getByText("Aç"));
    expect(onClick).toHaveBeenCalledWith({ a: "open_url", v: "https://projects.gg/kurallar" });
    // The game closes (or keeps, or waits) after a link too: the client is told of the press.
    const inputs = { amount: "16", note: "", fast: "false", item: "diamond" };
    expect(onAct).toHaveBeenCalledWith({ i: 2, inputs });
    fireEvent.click(screen.getByText("Kabul"));
    expect(onAct).toHaveBeenCalledWith({ i: 3, inputs });
    fireEvent.click(screen.getByText("Oyunda kapat"));
    expect(onAct).toHaveBeenCalledWith({ cancel: true });
  });

  it("marks a command and a link with the console's icon in the label's colour, one way out", () => {
    const raw = {
      ...RAW,
      buttons: [{ ...RAW.buttons[0], labelSt: [[6, "a"]] }, RAW.buttons[1], RAW.buttons[2]],
    };
    const { rerender } = show(
      <DialogCard dialog={safeDialog(raw) as Dialog} lang="tr" clock="17:30:00" onAct={vi.fn()} />,
    );
    const icon = (label: string) =>
      screen.getByText(label).closest("button")?.querySelector("svg.pl-con-act-icon");
    const green = lineRuns("Gönder", [[6, "a"]])?.[0]?.color;
    expect(green).toBeTruthy();
    expect(icon("Gönder")?.getAttribute("fill")).toBe(green);
    expect(icon("Kurallar")?.getAttribute("fill")).toBe("currentColor");
    expect(icon("Kabul")).toBeNull();
    expect(screen.getByText("Oyunda kapat")).not.toBeNull();
    const closing = { ...raw, buttons: [...raw.buttons, { i: 4, label: "Vazgeç", a: "close" }] };
    rerender(
      <TooltipProvider>
        <DialogCard
          dialog={safeDialog(closing) as Dialog}
          lang="tr"
          clock="17:30:00"
          onAct={vi.fn()}
        />
      </TooltipProvider>,
    );
    expect(screen.getByText("Vazgeç")).not.toBeNull();
    expect(screen.queryByText("Oyunda kapat")).toBeNull();
  });

  it("draws each input as the game does: a checkbox, radios or a list, a slider in its words", () => {
    const options = (n: number) =>
      Array.from({ length: n }, (_, i) => ({ id: `o${i}`, label: `Seçenek ${i}` }));
    const dialog = safeDialog({
      ...EXACT,
      inputs: [
        { ...RAW.inputs[0], fmt: "%s: %s" },
        RAW.inputs[2],
        { key: "few", kind: "option", label: "Takım", options: options(3), initial: "o1" },
        { key: "many", kind: "option", label: "Sunucu", options: options(6), initial: "o0" },
      ],
    }) as Dialog;
    show(<DialogCard dialog={dialog} lang="tr" clock="17:30:00" onAct={vi.fn()} />);
    expect(screen.getByRole("checkbox").getAttribute("aria-checked")).toBe("false");
    fireEvent.click(screen.getByRole("checkbox"));
    expect(screen.getByRole("checkbox").getAttribute("aria-checked")).toBe("true");
    // Up to five options as radio buttons, more as a list.
    expect(screen.getAllByRole("radio")).toHaveLength(3);
    expect(screen.getAllByRole("radio")[1]?.getAttribute("aria-checked")).toBe("true");
    expect(screen.getByText("Adet: 16")).not.toBeNull();
    expect(screen.getByRole("slider").getAttribute("aria-valuetext")).toBe("Adet: 16");
    fireEvent.keyDown(screen.getByRole("slider"), { key: "ArrowRight" });
    expect(screen.getByText("Adet: 17")).not.toBeNull();
  });

  it("pages a long list, says when the client cut it, and closes with the dialog's own exit", () => {
    const onAct = vi.fn();
    const buttons = Array.from({ length: 20 }, (_, i) => ({ i, label: `Sunucu ${i}`, a: "custom" }));
    show(
      <DialogCard
        dialog={
          safeDialog({ ...RAW, inputs: [], buttons, cut: true, exit: { label: "Vazgeç" } }) as Dialog
        }
        lang="tr"
        clock="17:30:00"
        onAct={onAct}
      />,
    );
    expect(screen.getByText("Sunucu 0")).not.toBeNull();
    expect(screen.queryByText("Sunucu 8")).toBeNull();
    expect(screen.getByText("1 / 3")).not.toBeNull();
    fireEvent.click(screen.getByLabelText("Sonraki sayfa"));
    expect(screen.getByText("Sunucu 8")).not.toBeNull();
    expect(screen.getByText("Pencerenin bir kısmı burada görünmüyor.")).not.toBeNull();
    fireEvent.click(screen.getByText("Vazgeç"));
    expect(onAct).toHaveBeenCalledWith({ cancel: true });
  });

  it("shows an item part with the game's tooltip unless the game hides it", () => {
    const item = { id: "minecraft:diamond", count: 3, label: { tr: "Elmas", en: "Diamond" } };
    show(
      <DialogCard
        dialog={safeDialog({ ...RAW, body: [{ text: "Ödülün", item }] }) as Dialog}
        lang="tr"
        clock="17:30:00"
        onAct={vi.fn()}
      />,
    );
    // The keyboard reaches its tooltip: the tile is a button that shows it.
    expect(screen.getByRole("button", { name: "Elmas" }).getAttribute("type")).toBe("button");
    expect(screen.getByText("Ödülün")).not.toBeNull();
    expect(screen.getByText("3")).not.toBeNull();
    cleanup();
    // The game draws no tooltip for it: a picture with its name.
    show(
      <DialogCard
        dialog={safeDialog({ ...RAW, body: [{ text: "Ödülün", item, noTip: true }] }) as Dialog}
        lang="tr"
        clock="17:30:00"
        onAct={vi.fn()}
      />,
    );
    expect(screen.getByRole("img", { name: "Elmas" })).not.toBeNull();
  });
});

describe("the game's waiting screen and menus", () => {
  it("holds its back button for five seconds, counting down", () => {
    vi.useFakeTimers();
    const onBack = vi.fn();
    show(<DialogWaiting lang="tr" onBack={onBack} />);
    expect(screen.getByRole("status").textContent).toBe("Sunucu bekleniyor");
    const back = () => screen.getByRole("button") as HTMLButtonElement;
    expect(back().textContent).toBe("Geri (5)");
    expect(back().disabled).toBe(true);
    for (let i = 0; i < 5; i++) {
      act(() => {
        vi.advanceTimersByTime(1000);
      });
    }
    expect(back().textContent).toBe("Geri");
    fireEvent.click(back());
    expect(onBack).toHaveBeenCalled();
  });

  it("offers the menus as the game labels them", () => {
    const onOpen = vi.fn();
    show(
      <DialogMenus
        menus={{ pause: { text: "Kurallar" }, quick: { text: "Hızlı Eylemler..." } }}
        lang="tr"
        onOpen={onOpen}
      />,
    );
    expect(screen.getByRole("group", { name: "Sunucu menüleri" })).not.toBeNull();
    fireEvent.click(screen.getByText("Hızlı Eylemler..."));
    expect(onOpen).toHaveBeenCalledWith("quick");
    cleanup();
    const { container } = show(<DialogMenus menus={{}} lang="tr" onOpen={onOpen} />);
    expect(container.textContent).toBe("");
  });
});
