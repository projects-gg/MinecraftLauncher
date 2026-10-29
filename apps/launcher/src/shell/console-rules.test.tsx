// @vitest-environment happy-dom
// CONSOLE-COLOR, TASTE-SWEEP's security read: the contract's one rule list, one test per rule (the
// same rules as MCC's ConsoleLooks, afk-host's looks.rs and the Worker's console.ts; the shared
// cases are the Worker's test-vectors/console-looks-v1.json). Invisible characters are built from
// their code points, so no file of ours hides one.
import {
  ConsoleActionCard,
  copiedPreview,
  domainParts,
  safeClick,
  safeHover,
  urlParts,
} from "@pl/ui/console";
import { safeInvCursor, safeInvItem, safeInvItems } from "@pl/ui/inventory";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

afterEach(cleanup);

const ch = (cp: number) => String.fromCodePoint(cp);
/** Every character the rule list forbids, one representative of each range end. */
const FORBIDDEN = [
  0x00, 0x1f, 0x7f, 0x85, 0x9f, 0xad, 0x34f, 0x61c, 0x115f, 0x1160, 0x180e, 0x200b, 0x200c, 0x200d,
  0x200e, 0x200f, 0x2028, 0x2029, 0x202a, 0x202e, 0x2060, 0x2064, 0x2066, 0x2069, 0x3164, 0xfeff,
  0xffa0, 0xe0000, 0xe0041, 0xe007f,
];

describe("a click's value", () => {
  it.each(FORBIDDEN)("refuses U+%s anywhere in it", (cp) => {
    for (const a of ["run_command", "suggest_command", "copy_to_clipboard"]) {
      const v = a === "run_command" ? `/pay${ch(cp)} LavaPanda_7 1` : `a${ch(cp)}b`;
      expect(safeClick({ a, v }), `${a} U+${cp.toString(16)}`).toBeNull();
    }
    expect(safeClick({ a: "open_url", v: `https://projects.gg/a${ch(cp)}b` })).toBeNull();
    // A hover's text refuses them too (its line break aside).
    if (cp !== 0x0a) expect(safeHover({ text: `a${ch(cp)}b` })).toBeNull();
  });

  it("keeps the characters players type", () => {
    expect(
      safeClick({ a: "suggest_command", v: "/msg Kaan_07 Selam, nasılsın? 🙂" }),
    ).not.toBeNull();
    expect(safeHover({ text: "Satır 1\nSatır 2" })).not.toBeNull();
  });
});

describe("a link's address", () => {
  const no = [
    ["sign-in before the host", "https://projects.gg@evil.example/"],
    ["a user and a password", "https://user:pass@projects.gg/"],
    ["a look-alike Cyrillic letter", `https://proj${ch(0x435)}cts.gg/`],
    ["punycode", "https://xn--prjects-6ve.gg/"],
    ["a punycode label anywhere", "https://www.xn--80ak6aa92e.com/"],
    ["an IPv4 address", "https://192.168.1.1/"],
    ["a loopback address with a port", "https://127.0.0.1:8080/"],
    ["an IPv6 address", "https://[::1]/"],
    ["a number for a host", "https://2130706433/"],
    ["a hex IPv4", "https://0x7f.0.0.1/"],
    ["localhost", "https://localhost/"],
    ["a .localhost name", "https://panel.localhost/"],
    ["a single label", "https://intranet/"],
    ["a port of letters", "https://projects.gg:abc/"],
    ["an empty label", "https://projects..gg/"],
    ["a trailing dot", "https://projects.gg./"],
    ["plain http", "http://projects.gg/"],
    ["upper-case scheme", "HTTPS://projects.gg/"],
    ["a space", "https://projects.gg/a b"],
    ["U+FEFF", `https://projects.gg${ch(0xfeff)}/`],
  ] as const;
  it.each(no)("refuses %s", (_name, v) => {
    expect(urlParts(v)).toBeNull();
    expect(safeClick({ a: "open_url", v })).toBeNull();
  });

  it("opens a plain https address and splits it at its host", () => {
    expect(urlParts("https://forum.projects.gg/t/etkinlik?sayfa=2#ust")).toEqual({
      host: "forum.projects.gg",
      rest: "/t/etkinlik?sayfa=2#ust",
    });
    expect(urlParts("https://projects.gg:8443")).toEqual({ host: "projects.gg:8443", rest: "" });
    expect(safeClick({ a: "open_url", v: "https://Projects.GG/kurallar" })?.v).toBe(
      "https://Projects.GG/kurallar",
    );
  });

  const card = (v: string) =>
    render(
      <ConsoleActionCard
        click={{ a: "open_url", v }}
        lang="tr"
        clock="17:25:22"
        onConfirm={() => undefined}
        onCancel={() => undefined}
      />,
    );

  it("shows the registrable domain alone, first and unbroken, the whole address under it", () => {
    card("https://forum.projects.gg/t/etkinlik");
    const domain = screen.getByText("projects.gg");
    expect(domain.className).toContain("font-semibold");
    expect(domain.className).toContain("whitespace-nowrap");
    expect(screen.getByText("forum.projects.gg/t/etkinlik").className).toContain("text-ash-400");
  });

  it("shows the real domain of a host that starts with another site's name (TASTE-SWEEP)", () => {
    card("https://forum.projects.gg.giris-yap.example/");
    expect(screen.getByText("giris-yap.example").className).toContain("font-semibold");
    expect(screen.queryByText("projects.gg")).toBeNull();
    expect(domainParts("forum.projects.gg.evil.example")).toEqual({
      sub: "forum.projects.gg.",
      domain: "evil.example",
    });
    expect(domainParts("forum.projects.gg")).toEqual({ sub: "forum.", domain: "projects.gg" });
    // A country's generic second level: three labels.
    expect(domainParts("www.kpss.com.tr").domain).toBe("kpss.com.tr");
    expect(domainParts("shop.example.co.uk:8443").domain).toBe("example.co.uk");
    expect(domainParts("projects.gg").sub).toBe("");
  });
});

describe("a copy's toast", () => {
  it("shows what was copied, at most 60 units, its start and its end", () => {
    expect(copiedPreview("PROMO-2026")).toBe("PROMO-2026");
    const long = `${"a".repeat(50)}${"b".repeat(50)}`;
    expect(copiedPreview(long)).toBe(`${"a".repeat(28)} … ${"b".repeat(28)}`);
    expect(copiedPreview(long).length).toBeLessThanOrEqual(60);
    // White space cannot push a command out of sight (pasting would bring it).
    const padded = `mc.server.net${" ".repeat(80)}/op Kaan_07`;
    expect(copiedPreview(padded)).toBe("mc.server.net /op Kaan_07");
    // Never half a character, at either cut.
    const emoji = `${"x".repeat(27)}🙂${"y".repeat(30)}🙂${"z".repeat(27)}`;
    const shown = copiedPreview(emoji);
    expect(shown.startsWith("x".repeat(27))).toBe(true);
    expect(shown.endsWith("z".repeat(27))).toBe(true);
    expect(shown).not.toMatch(/[\uD800-\uDBFF](?![\uDC00-\uDFFF])|(?<![\uD800-\uDBFF])[\uDC00-\uDFFF]/);
  });
});

describe("a machine's item", () => {
  const ITEM = {
    slot: 3,
    id: "minecraft:diamond_sword",
    count: 1,
    label: { tr: "Elmas Kılıç", en: "Diamond Sword" },
    name: "Keskin Kılıç",
    nameStyle: [[12, "b"]],
    lore: ["Sezon ödülü", ""],
    loreStyle: [[[11, "7"]], null],
    ench: [{ id: "minecraft:sharpness", lvl: 5, label: { tr: "Keskinlik", en: "Sharpness" } }],
    skin: "46acd06e8483b176e8ea39fc12fe105eb3a2a4970f5100057e9d84d4b60bdfa7",
  };

  it("keeps an item in shape as it is", () => {
    expect(safeInvItem(ITEM)).toEqual(ITEM);
    expect(safeInvItems([ITEM, { id: "Not An Id" }])).toHaveLength(1);
    expect(safeInvCursor({ ...ITEM })).not.toHaveProperty("slot");
  });

  it("drops an item without its id, label or a sane count", () => {
    expect(safeInvItem({ ...ITEM, id: "<b>sword</b>" })).toBeNull();
    expect(safeInvItem({ ...ITEM, label: { tr: "x" } })).toBeNull();
    expect(safeInvItem({ ...ITEM, count: 0 })).toBeNull();
    expect(safeInvItem({ ...ITEM, slot: -1 })).toBeNull();
    expect(safeInvItem("minecraft:stone")).toBeNull();
  });

  it("drops an odd optional field alone", () => {
    const named = safeInvItem({ ...ITEM, name: `a${ch(0x202e)}b` });
    expect(named).not.toHaveProperty("name");
    expect(named).not.toHaveProperty("nameStyle");
    expect(safeInvItem({ ...ITEM, nameStyle: [[3, "b"]] })).not.toHaveProperty("nameStyle");
    expect(safeInvItem({ ...ITEM, lore: ["ok", 5] })).not.toHaveProperty("lore");
    expect(
      safeInvItem({ ...ITEM, lore: Array.from({ length: 33 }, () => "x") }),
    ).not.toHaveProperty("lore");
    expect(safeInvItem({ ...ITEM, skin: "https://evil.example/skin.png" })).not.toHaveProperty(
      "skin",
    );
    expect(
      safeInvItem({ ...ITEM, ench: [{ id: "x", lvl: 1, label: ITEM.label }] }),
    ).not.toHaveProperty("ench");
    expect(safeInvItem({ ...ITEM, name: "y".repeat(257) })).not.toHaveProperty("name");
  });
});
