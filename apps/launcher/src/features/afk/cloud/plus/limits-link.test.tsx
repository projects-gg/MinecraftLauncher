// @vitest-environment happy-dom
// "Sınırlara git": only beside a line about a limit (an approval's reason as the card picks it),
// and it goes to that limit.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import "@/lib/i18n";
import { LimitLine } from "./limits-link";
import { limitReason } from "./model";

const ignore = () => undefined;

afterEach(() => cleanup());

describe("Sınırlara git", () => {
  it("stands beside a line about a limit and goes to it", () => {
    const went: string[] = [];
    render(<LimitLine onGo={() => went.push("limits")}>Onay sınırının üstünde</LimitLine>);
    expect(screen.getByText("Onay sınırının üstünde")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Sınırlara git" }));
    expect(went).toEqual(["limits"]);
  });

  it.each<[string, boolean]>([
    ["above", true],
    ["new_counterpart", true],
    ["daily", true],
    ["budget", true],
    ["authority", false],
    ["price_unusual", false],
    ["other", false],
  ])("on an approval's %s line: %s", (reason, shown) => {
    render(<LimitLine onGo={limitReason([reason]) ? ignore : null}>{reason}</LimitLine>);
    expect(screen.getByText(reason)).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Sınırlara git" }) !== null).toBe(shown);
  });
});
