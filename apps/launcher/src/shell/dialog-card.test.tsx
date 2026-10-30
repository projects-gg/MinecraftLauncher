// @vitest-environment happy-dom
// DIALOG-CARD: a server's dialog checked as the contract writes it, the command its inputs build
// as the client builds it, and the card that never runs a command the player has not seen.
import {
  type Dialog,
  DialogCard,
  dialogCommand,
  dialogRefusal,
  initialValues,
  lineRuns,
  numberText,
  safeDialog,
  snap,
} from "@pl/ui/console";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(cleanup);

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

describe("a server's dialog", () => {
  it("reads only as the contract writes it", () => {
    const dialog = safeDialog(RAW) as Dialog;
    expect(dialog).not.toBeNull();
    expect(dialog.inputs.map((input) => input.kind)).toEqual(["range", "text", "bool", "option"]);
    expect(dialog.buttons.map((button) => button.a)).toEqual(["run_command", "open_url", "custom"]);
    const odd = (patch: Record<string, unknown>) => safeDialog({ ...RAW, ...patch });
    expect(odd({ title: "a\u202eb" })).toBeNull();
    expect(odd({ title: "x".repeat(129) })).toBeNull();
    expect(odd({ buttons: [{ i: 1, label: "x", a: "run_command", v: "trade" }] })).toBeNull();
    expect(
      odd({ buttons: [{ i: 1, label: "x", a: "open_url", v: "http://projects.gg" }] }),
    ).toBeNull();
    expect(odd({ buttons: [{ i: 1, label: "x", a: "open_file", v: "C:/x" }] })).toBeNull();
    expect(
      odd({ buttons: Array.from({ length: 13 }, (_, i) => ({ i, label: "x", a: "close" })) }),
    ).toBeNull();
    expect(odd({ inputs: [{ key: "Bad Key", kind: "text", label: "x", max: 8 }] })).toBeNull();
    expect(odd({ inputs: [{ key: "n", kind: "text", label: "x", max: 999 }] })).toBeNull();
    expect(odd({ body: [{ text: "y".repeat(20_000) }] })).toBeNull();
    // A part's looks that do not cover it go; the part stays.
    expect(safeDialog({ ...RAW, titleSt: [[2, "6"]] })?.titleSt).toBeNull();
    // The rule list as for a console line; a body part is text only.
    const zw = String.fromCodePoint(0x200b);
    expect(odd({ title: `Ta${zw}kas` })).toBeNull();
    expect(odd({ body: [{ item: { id: "minecraft:dirt" } }] })).toBeNull();
    expect(
      odd({ buttons: [{ i: 1, label: "x", a: "open_url", v: "https://xn--prjects-6ve.gg/" }] }),
    ).toBeNull();
    // The client's keys may hold capitals; a multiline text's initial may hold line breaks.
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

  it("builds a command as the client does: in order, escaped text, the switch's words", () => {
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
    // Out of the input's rules: no command at all.
    expect(dialogCommand(dialog, button, { ...values, item: "netherite" })).toBeNull();
    expect(dialogCommand(dialog, button, { ...values, note: "x".repeat(33) })).toBeNull();
    expect(dialogCommand(dialog, button, { ...values, amount: 65 })).toBeNull();
    expect(dialogCommand(dialog, button, { ...values, note: "a\u0000" })).toBeNull();
    // A configuration-phase dialog runs no command.
    expect(dialogCommand({ ...dialog, play: false }, button, values)).toBeNull();
  });

  it("writes numbers and snaps sliders as the client does", () => {
    expect(numberText(16)).toBe("16");
    expect(numberText(0.1 + 0.2)).toBe("0.3");
    const range = {
      key: "k",
      kind: "range" as const,
      label: "",
      start: 0,
      end: 1,
      step: 0.25,
      initial: 0,
    };
    expect(snap(range, 0.6)).toBe(0.5);
    expect(snap(range, 2)).toBe(1);
  });

  it("says why a press was refused without a detail", () => {
    expect(dialogRefusal("stale", "tr")).toBe("Sunucu penceresi değişti.");
    expect(dialogRefusal("cmd", "tr")).toBe("Bu düğme şu an çalışmadı.");
    expect(dialogRefusal(undefined, "en")).toBe("This button didn't work just now.");
  });
});

describe("DialogCard", () => {
  it("shows a refused press under its buttons", () => {
    render(
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
    const { rerender } = render(
      <DialogCard dialog={{ ...dialog, play: false }} lang="tr" clock="17:30:00" onAct={vi.fn()} />,
    );
    expect(screen.getByText("Gönder").closest("button")?.disabled).toBe(true);
    expect(screen.getByText("Komut düğmeleri oyuna girince çalışır.")).not.toBeNull();
    rerender(<DialogCard dialog={dialog} lang="tr" clock="17:30:00" onAct={vi.fn()} />);
    expect(screen.getByText("Gönder").closest("button")?.disabled).toBe(false);
    expect(screen.queryByText("Komut düğmeleri oyuna girince çalışır.")).toBeNull();
  });

  it("asks before a command runs, with the command its inputs build", () => {
    const onAct = vi.fn();
    render(
      <DialogCard dialog={safeDialog(RAW) as Dialog} lang="tr" clock="17:30:00" onAct={onAct} />,
    );
    fireEvent.click(screen.getByText("Gönder"));
    expect(onAct).not.toHaveBeenCalled();
    expect(screen.getByText("Bu komut çalışsın mı?")).not.toBeNull();
    expect(screen.getByText("/trade send diamond 16 hayir")).not.toBeNull();
    expect(screen.getByText("Sunucu penceresinden, 17:30:00")).not.toBeNull();
    fireEvent.click(screen.getByText("Çalıştır"));
    // The empty note leaves the template's space: the command is exactly what the client builds.
    expect(onAct).toHaveBeenCalledWith({
      i: 1,
      inputs: { amount: "16", note: "", fast: "false", item: "diamond" },
      cmd: "/trade send diamond 16 hayir ",
    });
  });

  it("asks before a link opens and sends the game's own buttons as they are", () => {
    const onAct = vi.fn();
    const onClick = vi.fn();
    render(
      <DialogCard
        dialog={safeDialog(RAW) as Dialog}
        lang="tr"
        clock="17:30:00"
        onAct={onAct}
        onClick={onClick}
      />,
    );
    fireEvent.click(screen.getByText("Kurallar"));
    // The link card: the registrable domain alone in bold, the whole address muted under it.
    expect(screen.getByText("projects.gg").className).toContain("font-semibold");
    expect(screen.getByText("projects.gg/kurallar")).not.toBeNull();
    fireEvent.click(screen.getByText("Aç"));
    expect(onClick).toHaveBeenCalledWith({ a: "open_url", v: "https://projects.gg/kurallar" });
    fireEvent.click(screen.getByText("Kabul"));
    expect(onAct).toHaveBeenCalledWith({
      i: 3,
      inputs: { amount: "16", note: "", fast: "false", item: "diamond" },
    });
    fireEvent.click(screen.getByText("Oyunda kapat"));
    expect(onAct).toHaveBeenCalledWith({ cancel: true });
  });

  it("marks a command and a link with the console's icon in the label's colour, one way out", () => {
    const raw = {
      ...RAW,
      buttons: [{ ...RAW.buttons[0], labelSt: [[6, "a"]] }, RAW.buttons[1], RAW.buttons[2]],
    };
    const { rerender } = render(
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
    // The window's own close button: the card's close would be a second way out of it.
    const closing = { ...raw, buttons: [...raw.buttons, { i: 4, label: "Vazgeç", a: "close" }] };
    rerender(
      <DialogCard
        dialog={safeDialog(closing) as Dialog}
        lang="tr"
        clock="17:30:00"
        onAct={vi.fn()}
      />,
    );
    expect(screen.getByText("Vazgeç")).not.toBeNull();
    expect(screen.queryByText("Oyunda kapat")).toBeNull();
  });
});
