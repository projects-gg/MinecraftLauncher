// @vitest-environment happy-dom
// "Para birimi": nothing while Studio's switch is off, the profile's own name read-only, else the
// player's own field, with one line under it (the hint, or that the name cannot be used).
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import "@/lib/i18n";
import { CurrencyField } from "./currency-field";

const ignore = () => {};

/** The field, after checking that its words name it. */
function fieldOf(container: HTMLElement): HTMLInputElement {
  const field = container.querySelector("input") as HTMLInputElement;
  expect(screen.getByText("Para birimi").getAttribute("for")).toBe(field.id);
  return field;
}

afterEach(() => cleanup());

describe("the money's name field (Para birimi)", () => {
  it("shows nothing while Studio's switch is off", () => {
    const { container } = render(
      <CurrencyField state={{ kind: "hidden" }} value="TL" refused={false} onChange={ignore} />,
    );
    expect(container.innerHTML).toBe("");
  });

  it("shows the profile's own name read-only, with the hint", () => {
    const fixed = { kind: "fixed", label: "ripple" } as const;
    const { container } = render(
      <CurrencyField state={fixed} value="TL" refused={false} onChange={ignore} />,
    );
    const field = fieldOf(container);
    expect(field.value).toBe("ripple");
    expect(field.readOnly).toBe(true);
    expect(screen.getByText("Sunucundaki paranın adı.")).toBeTruthy();
  });

  it("takes the player's own name and says when it cannot be used", () => {
    const typed: string[] = [];
    const own = { kind: "own" } as const;
    const { container, rerender } = render(
      <CurrencyField state={own} value="" refused={false} onChange={(text) => typed.push(text)} />,
    );
    const field = fieldOf(container);
    expect(field.readOnly).toBe(false);
    fireEvent.change(field, { target: { value: "TL" } });
    expect(typed).toEqual(["TL"]);
    expect(screen.queryByRole("alert")).toBeNull();
    rerender(<CurrencyField state={own} value="TL2" refused onChange={ignore} />);
    expect(screen.getByRole("alert").textContent).toBe("Bu ad kullanılamaz.");
    expect(field.getAttribute("aria-invalid")).toBe("true");
  });
});
