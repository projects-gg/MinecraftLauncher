import { describe, expect, it } from "vitest";
import { welcomeHome } from "./welcome";

describe("welcomeHome", () => {
  it("greets without announcing anything", () => {
    const home = welcomeHome();
    expect(home.hero.title).toBe("Projects'e hoş geldin");
    expect(home.hero.subtitle).toBe("Oyna'ya bas, sunucuda görüşürüz.");
    expect(home.hero.newsId).toBeUndefined();
    expect(home.hero.ctaUrl).toBeUndefined();
    expect(home.banners).toEqual([]);
    expect(home.events).toEqual([]);
    expect(home.news).toEqual([]);
    expect(home.patchNotes).toBeUndefined();
    expect(home.maintenance).toBeUndefined();
    expect(home.offline).toBe(false);
  });
});
