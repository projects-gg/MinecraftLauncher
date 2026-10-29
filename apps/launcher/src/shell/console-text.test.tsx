// @vitest-environment happy-dom
// CONSOLE-COLOR: a console line in the game's looks, its hovers and clicks, and the card a clicked
// command or link waits in.
import { ConsoleActionCard, ConsoleDirection, ConsoleText } from "@pl/ui/console";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

afterEach(cleanup);

const LINE = "[Parti] FrostyOtter88 seni partiye çağırıyor [Kabul et]";
const ST = [
  [8, "9"],
  [37, "7"],
  [10, "al", 0],
];
const EV = [
  {
    click: { a: "run_command", v: "/party accept FrostyOtter88" },
    hover: { text: "Partiye katılmak için tıkla", st: [[27, "a"]] },
  },
];

function line(props: Partial<Parameters<typeof ConsoleText>[0]> = {}) {
  return render(
    <p data-testid="line">
      <ConsoleText text={LINE} st={ST} ev={EV} lang="tr" {...props} />
    </p>,
  );
}

describe("ConsoleText", () => {
  it("draws the parts as plain text in their colours: a copy is the line", () => {
    line();
    const p = screen.getByTestId("line");
    expect(p.textContent).toBe(LINE);
    expect(p.innerHTML).not.toContain("<b");
    const accept = screen.getByText("[Kabul et]");
    expect(accept.style.fontWeight).toBe("700");
    expect(accept.style.color).not.toBe("");
  });

  it("never reads a server's text as markup", () => {
    const text = '<img src=x onerror="alert(1)">';
    render(
      <p data-testid="line">
        <ConsoleText text={text} st={[[text.length, "c"]]} lang="tr" />
      </p>,
    );
    const p = screen.getByTestId("line");
    expect(p.textContent).toBe(text);
    expect(p.querySelector("img")).toBeNull();
  });

  it("tells a part that does something from a server's underline", () => {
    const onClick = vi.fn();
    render(
      <p data-testid="line">
        <ConsoleText
          text="Eski fiyat 600 ripple [Kabul et]"
          st={[
            [11, "7"],
            [10, "an"],
            [1, ""],
            [10, "al", 0],
          ]}
          ev={EV}
          lang="tr"
          onClick={onClick}
        />
      </p>,
    );
    // The server's underline: thin and faint, no button, no icon.
    const price = screen.getByText("600 ripple");
    expect(price.style.textDecorationLine).toBe("underline");
    expect(price.style.textDecorationThickness).toBe("1px");
    expect(price.getAttribute("role")).toBeNull();
    // The command: a button with a small line icon (no words of its own: a copy stays the line).
    const accept = screen.getByRole("button", { name: "[Kabul et]" });
    expect(accept.querySelector("svg")).not.toBeNull();
    expect(screen.getByTestId("line").textContent).toBe("Eski fiyat 600 ripple [Kabul et]");
  });

  it("offers a click only where the console can send", () => {
    line();
    expect(screen.queryByRole("button")).toBeNull();
    cleanup();
    const onClick = vi.fn();
    line({ onClick });
    const accept = screen.getByRole("button", { name: "[Kabul et]" });
    fireEvent.click(accept);
    expect(onClick).toHaveBeenCalledWith({ a: "run_command", v: "/party accept FrostyOtter88" });
    fireEvent.keyDown(accept, { key: "Enter" });
    expect(onClick).toHaveBeenCalledTimes(2);
  });

  it("shows the server's hover as the game's tooltip while the pointer is on the part", () => {
    line();
    const accept = screen.getByText("[Kabul et]");
    fireEvent.pointerEnter(accept, { pointerType: "mouse", clientX: 40, clientY: 40 });
    const tip = document.body.querySelector(".pl-inv-tip");
    expect(tip).not.toBeNull();
    expect(tip?.querySelector("[data-text]")?.getAttribute("data-text")).toBe(
      "Partiye katılmak için tıkla",
    );
    fireEvent.pointerLeave(accept, { pointerType: "mouse" });
    expect(document.body.querySelector(".pl-inv-tip")).toBeNull();
  });

  it("opens a hover with a tap and closes it with the next tap elsewhere", () => {
    const hoverOnly = [{ hover: { text: "Sunucu duyurusu" } }];
    render(
      <p>
        <ConsoleText
          text="[Sunucu] Etkinlik"
          st={[
            [8, "6l", 0],
            [9, "e"],
          ]}
          ev={hoverOnly}
          lang="tr"
        />
      </p>,
    );
    const part = screen.getByText("[Sunucu]");
    fireEvent.pointerDown(part, { pointerType: "touch" });
    expect(document.body.querySelector(".pl-inv-tip")).not.toBeNull();
    fireEvent.pointerDown(document.body, { pointerType: "touch" });
    expect(document.body.querySelector(".pl-inv-tip")).toBeNull();
  });

  it("shows an item's hover as the inventory's tooltip when the host can draw it", () => {
    const ev = [
      { hover: { item: { id: "minecraft:diamond_sword", count: 1, name: "Keskin Kılıç" } } },
    ];
    const props = { text: "[Kılıç]", st: [[7, "b", 0]], ev, lang: "tr" as const };
    render(
      <p>
        <ConsoleText {...props} />
      </p>,
    );
    // No host to draw items: no hover is offered.
    fireEvent.pointerEnter(screen.getByText("[Kılıç]"), { pointerType: "mouse" });
    expect(document.body.querySelector(".pl-inv-tip")).toBeNull();
    cleanup();
    render(
      <p>
        <ConsoleText
          {...props}
          itemOf={(raw) => ({
            slot: 0,
            id: (raw as { id: string }).id,
            count: 1,
            name: (raw as { name: string }).name,
            label: { tr: "Elmas Kılıç", en: "Diamond Sword" },
          })}
        />
      </p>,
    );
    fireEvent.pointerEnter(screen.getByText("[Kılıç]"), { pointerType: "mouse" });
    const tip = document.body.querySelector(".pl-inv-tip");
    expect(tip?.querySelector("[data-text]")?.getAttribute("data-text")).toBe("Keskin Kılıç");
  });

  it("shows the line as today when its parts are not the contract's", () => {
    render(
      <p data-testid="line">
        <ConsoleText text="selam" st={[[4, "a"]]} lang="tr">
          <span data-testid="today">selam</span>
        </ConsoleText>
      </p>,
    );
    expect(screen.getByTestId("today")).not.toBeNull();
  });
});

describe("ConsoleActionCard", () => {
  it("shows the exact command and whose line it came from, the safe choice focused", () => {
    const onConfirm = vi.fn();
    const onCancel = vi.fn();
    render(
      <ConsoleActionCard
        click={{ a: "run_command", v: "/party accept FrostyOtter88" }}
        lang="tr"
        who={null}
        clock="17:25:20"
        onConfirm={onConfirm}
        onCancel={onCancel}
      />,
    );
    expect(screen.getByText("Bu komut çalışsın mı?")).not.toBeNull();
    expect(screen.getByText("/party accept FrostyOtter88")).not.toBeNull();
    expect(screen.getByText("Sunucu mesajından, 17:25:20")).not.toBeNull();
    expect(document.activeElement?.textContent).toBe("Vazgeç");
    fireEvent.keyDown(screen.getByRole("group"), { key: "Escape" });
    expect(onCancel).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByText("Çalıştır"));
    expect(onConfirm).toHaveBeenCalledTimes(1);
  });

  it("names the player of a chat line and asks before a link opens", () => {
    render(
      <ConsoleActionCard
        click={{ a: "open_url", v: "https://forum.projects.gg/t/etkinlik" }}
        lang="en"
        who="LavaPanda_7"
        clock="17:25:22"
        onConfirm={() => undefined}
        onCancel={() => undefined}
      />,
    );
    expect(screen.getByText("Open this link?")).not.toBeNull();
    // The host alone, first and in bold (what a player reads to know where a link goes), the
    // rest of the address muted under it.
    expect(screen.getByText("forum.projects.gg").className).toContain("font-semibold");
    expect(screen.getByText("/t/etkinlik").className).toContain("text-ash-400");
    expect(screen.getByText("From LavaPanda_7's message, 17:25:22")).not.toBeNull();
    expect(screen.getByText("Open")).not.toBeNull();
  });
});

describe("ConsoleDirection", () => {
  it("marks a private line to you and your own reply with the console's icons", () => {
    const { container } = render(
      <p>
        <ConsoleDirection dir="in" lang="tr" />
        <ConsoleDirection dir="out" lang="tr" />
        <ConsoleDirection dir="sideways" lang="tr" />
        <ConsoleDirection dir={undefined} lang="tr" />
      </p>,
    );
    expect(screen.getByRole("img", { name: "Sana gelen özel mesaj" })).not.toBeNull();
    const out = screen.getByRole("img", { name: "Senin özel mesajın" });
    expect(out.textContent).toBe("Sen");
    expect(container.querySelectorAll("span[role=img]").length).toBe(2);
    // No glyph of the server's: our icons are drawn.
    expect(container.textContent).not.toContain("✉");
  });
});
