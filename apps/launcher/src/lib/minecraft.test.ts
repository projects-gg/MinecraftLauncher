import { describe, expect, it } from "vitest";
import type { MinecraftNewsItem } from "@/lib/api";
import { columnFill, liveMinecraft, minecraftText } from "./minecraft";

const entry = (version: string, en?: MinecraftNewsItem["en"]): MinecraftNewsItem => ({
  id: `minecraft-${version}`,
  version,
  kind: "release",
  title: `Minecraft ${version} çıktı`,
  summary: "Yeni biyomlar ve kamp eşyaları.",
  body: "## Öne çıkanlar\n\n- **Dappled Forest**",
  publishedAt: "2026-09-15T11:23:02Z",
  expiresAt: "2026-10-30T11:23:02Z",
  url: "https://www.minecraft.net/article/minecraft-java-edition-26-3",
  en,
});

describe("Minecraft news", () => {
  it("speaks the interface's language when the bot wrote it", () => {
    const item = entry("26.3", {
      title: "Minecraft 26.3 is out",
      summary: "New biomes and camping gear.",
      body: "## Highlights\n\n- The **Dappled Forest**",
    });
    expect(minecraftText(item, "tr").title).toBe("Minecraft 26.3 çıktı");
    expect(minecraftText(item, "en")).toEqual({
      title: "Minecraft 26.3 is out",
      summary: "New biomes and camping gear.",
      body: "## Highlights\n\n- The **Dappled Forest**",
    });
    // A missing English summary or body falls back to the base text; no English at all, too.
    const titleOnly = entry("26.2", { title: "Minecraft 26.2 is out" });
    expect(minecraftText(titleOnly, "en")).toEqual({
      title: "Minecraft 26.2 is out",
      summary: "Yeni biyomlar ve kamp eşyaları.",
      body: "## Öne çıkanlar\n\n- **Dappled Forest**",
    });
    expect(minecraftText(entry("26.1"), "en").title).toBe("Minecraft 26.1 çıktı");
  });

  it("fills only the rows Projects news leaves empty", () => {
    const items = ["26.3", "26.1.2", "26.1"].map((v) => entry(v));
    expect(columnFill(0, items).map((i) => i.version)).toEqual(["26.3", "26.1.2", "26.1"]);
    expect(columnFill(1, items).map((i) => i.version)).toEqual(["26.3", "26.1.2", "26.1"]);
    expect(columnFill(2, items).map((i) => i.version)).toEqual(["26.3", "26.1.2"]);
    expect(columnFill(3, items).map((i) => i.version)).toEqual(["26.3"]);
    expect(columnFill(4, items)).toEqual([]);
    expect(columnFill(5, items)).toEqual([]);
  });

  it("drops an entry once it expires", () => {
    const items = [entry("26.3"), { ...entry("26.2"), expiresAt: "2026-10-18T12:00:00Z" }];
    const at = Date.parse("2026-10-18T12:00:00Z");
    expect(liveMinecraft(items, at - 1).length).toBe(2);
    expect(liveMinecraft(items, at).map((i) => i.version)).toEqual(["26.3"]);
    expect(liveMinecraft(undefined, at)).toEqual([]);
  });
});
