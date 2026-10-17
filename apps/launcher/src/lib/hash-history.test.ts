import { describe, expect, it } from "vitest";
import { createHashRouteHistory, hashRoute } from "./hash-history";

/** Just enough of a browser window for the router's history. */
function fakeWindow(url: string) {
  const location = { pathname: "/", search: "", hash: "" };
  const visit = (href: string) => {
    const next = new URL(href, `http://preview${location.pathname}`);
    location.pathname = next.pathname;
    location.search = next.search;
    location.hash = next.hash;
  };
  visit(url);
  const history = {
    state: null as unknown,
    length: 1,
    pushState(state: unknown, _title: string, href?: string) {
      history.state = state;
      history.length += 1;
      if (href) visit(href);
    },
    replaceState(state: unknown, _title: string, href?: string) {
      history.state = state;
      if (href) visit(href);
    },
    back() {},
    forward() {},
    go() {},
  };
  const win = { location, history, addEventListener() {}, removeEventListener() {} };
  return win as typeof win & Window;
}

describe("the route in the address", () => {
  it("reads the path and the query from the hash", () => {
    expect(hashRoute("#/library/yaratici?tab=mods")).toEqual({
      href: "/library/yaratici?tab=mods",
      pathname: "/library/yaratici",
      search: "?tab=mods",
      hash: "",
    });
    expect(hashRoute("#/library/yaratici?tab=mods#notes")).toMatchObject({
      pathname: "/library/yaratici",
      search: "?tab=mods",
      hash: "#notes",
    });
  });

  it("starts at home without a hash", () => {
    expect(hashRoute("")).toMatchObject({ href: "/", pathname: "/", search: "" });
    expect(hashRoute("#")).toMatchObject({ href: "/", pathname: "/" });
    expect(hashRoute("#library")).toMatchObject({ pathname: "/library" });
  });

  it("never reads a protocol-relative route or control characters", () => {
    expect(hashRoute("#//elsewhere.example/x").pathname).toBe("/elsewhere.example/x");
    expect(hashRoute("#/\\elsewhere.example").pathname).toBe("/elsewhere.example");
    expect(hashRoute(`#/library/a${String.fromCharCode(10)}?tab=mods`).search).toBe("?tab=mods");
  });
});

describe("the router's history", () => {
  it("opens a deep link to a tab, whatever the page's own query string", () => {
    for (const url of [
      "http://preview/#/library/yaratici?tab=mods",
      "http://preview/?emptyContent=1#/library/yaratici?tab=mods",
    ]) {
      const { location } = createHashRouteHistory(fakeWindow(url));
      expect(location.pathname).toBe("/library/yaratici");
      expect(location.search).toBe("?tab=mods");
    }
  });

  it("keeps the page's query string when the app moves", () => {
    const win = fakeWindow("http://preview/?emptyContent=1#/library/yaratici?tab=mods");
    const history = createHashRouteHistory(win);
    history.push("/library/yaratici?tab=worlds");
    history.flush();
    expect(win.location.search).toBe("?emptyContent=1");
    expect(win.location.hash).toBe("#/library/yaratici?tab=worlds");
    expect(history.location.search).toBe("?tab=worlds");
  });
});
