// @vitest-environment happy-dom
// The shared menu's named groups and its keys for choices (APG; SQOL-U2): a group's name is never a
// row and names the rows under it up to the next line; Space checks a choice and leaves the menu
// open, Enter checks it and closes the menu.
import { Menu, type MenuItem, setAppReducedMotion } from "@pl/ui";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { useState } from "react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";

beforeAll(() => setAppReducedMotion(true));
afterAll(() => setAppReducedMotion(false));
afterEach(cleanup);

/** An open menu that says when it opens or closes. */
function Harness({
  items,
  onOpenChange,
}: {
  items: Array<MenuItem | null>;
  onOpenChange: (open: boolean) => void;
}) {
  const [open, setOpen] = useState(true);
  return (
    <Menu
      items={items}
      open={open}
      onOpenChange={(next) => {
        setOpen(next);
        onOpenChange(next);
      }}
      label="Seçenekler"
      trigger={<button type="button">Aç</button>}
    />
  );
}

const nothing = () => {};

describe("the shared menu's groups and choices", () => {
  it("names a group's rows and never makes its name a row", () => {
    render(
      <Harness
        onOpenChange={nothing}
        items={[
          { label: "Kare Hızı", heading: true, onSelect: nothing },
          { label: "15 fps", radio: true, checked: false, onSelect: nothing },
          { label: "30 fps", radio: true, checked: true, onSelect: nothing },
          null,
          { label: "İmleci Göster", checked: true, onSelect: nothing },
        ]}
      />,
    );
    const menu = screen.getByRole("menu", { name: "Seçenekler" });
    const group = within(menu).getByRole("group", { name: "Kare Hızı" });
    expect(
      within(group)
        .getAllByRole("menuitemradio")
        .map((row) => row.textContent),
    ).toEqual(["15 fps", "30 fps"]);
    expect(within(menu).queryByRole("menuitem", { name: "Kare Hızı" })).toBeNull();
    // The line ends the group: the switch after it is not one of its rows.
    expect(within(group).queryByRole("menuitemcheckbox")).toBeNull();
    expect(within(menu).getByRole("menuitemcheckbox", { name: "İmleci Göster" })).toBeTruthy();
  });

  it("checks a choice with Space and stays open; Enter checks it and closes", () => {
    const pick = vi.fn();
    const openChange = vi.fn();
    render(
      <Harness
        onOpenChange={openChange}
        items={[
          { label: "Kare Hızı", heading: true, onSelect: nothing },
          { label: "15 fps", radio: true, checked: false, onSelect: pick },
        ]}
      />,
    );
    const row = screen.getByRole("menuitemradio", { name: "15 fps" });
    fireEvent.keyDown(row, { key: " " });
    expect(pick).toHaveBeenCalledTimes(1);
    expect(openChange).not.toHaveBeenCalledWith(false);
    expect(screen.queryByRole("menu")).not.toBeNull();
    fireEvent.keyDown(row, { key: "Enter" });
    expect(pick).toHaveBeenCalledTimes(2);
    expect(openChange).toHaveBeenCalledWith(false);
  });
});
