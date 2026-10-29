// @vitest-environment happy-dom
// A link's card names the site once: its registrable domain alone, in bold, from the address itself
// (never the page's own name for itself), then the title and the description; never the address
// again, which the message's text above already shows (CONSOLE-COLOR's console card's rule).
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import "@/lib/i18n";
import type { ChatCard } from "@/lib/api/social-types";
import { LinkCard } from "./link-ui";

afterEach(() => cleanup());

const link = {
  start: 0,
  end: 52,
  url: "https://forum.projects.gg.kotu.example/konu/123?ref=x",
  host: "forum.projects.gg.kotu.example",
  ok: true,
  trusted: false,
};

const card = (extra: Partial<ChatCard>): ChatCard => ({
  state: "ready",
  url: link.url,
  site: "Projects",
  icon: "globe",
  kind: "page",
  ...extra,
});

describe("a link's card", () => {
  it("names the domain once, in bold, then the title and the description", () => {
    const { container } = render(
      <LinkCard
        link={link}
        card={card({ title: "Yeni sezon", desc: "Bütün ödüller", author: "Kanal" })}
        waiting={false}
      />,
    );
    const text = container.textContent ?? "";
    expect(text).toContain("kotu.example");
    expect(text).toContain("Yeni sezon");
    expect(text).toContain("Bütün ödüller");
    // Neither the page's own name for itself nor any part of the address beyond its domain.
    expect(text).not.toContain("Projects");
    expect(text).not.toContain("projects.gg");
    expect(text).not.toContain("/konu");
    expect(text).not.toContain("Kanal");
    const bold = [...container.querySelectorAll(".font-semibold")].map((e) => e.textContent);
    expect(bold).toContain("kotu.example");
  });

  it("shows only the domain when the card has nothing more", () => {
    const { container } = render(
      <LinkCard link={link} card={card({ state: "plain" })} waiting={false} />,
    );
    const text = container.textContent ?? "";
    expect(text).toContain("kotu.example");
    expect(text).not.toContain("/konu");
    expect(text).not.toContain("forum.");
  });
});
