// @vitest-environment happy-dom
// SMALLS-L3: the AFK address field keeps a pasted link while it is typed, and once a save is taken
// or the edits are dropped (the editor's `revision`) it shows the address that was kept.
import { cleanup, fireEvent, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { AddressField } from "./controls";

afterEach(() => cleanup());

type OnChange = (host: string, port: string) => void;
const ignore: OnChange = () => {};

function field(host: string, port: number | string, revision: number, onChange = ignore) {
  return (
    <AddressField
      id="afk-host"
      label="Sunucu adresi"
      host={host}
      port={port}
      revision={revision}
      onChange={onChange}
    />
  );
}

describe("the AFK address field", () => {
  it("keeps a pasted link while typing and shows the kept address after a save", () => {
    const typed: Array<[string, string]> = [];
    const { container, rerender } = render(
      field("play.projects.gg", 0, 0, (host, port) => typed.push([host, port])),
    );
    const input = container.querySelector("input") as HTMLInputElement;
    expect(input.value).toBe("play.projects.gg");

    fireEvent.change(input, { target: { value: "https://mc.sunucu.net:25565" } });
    expect(typed.at(-1)).toEqual(["mc.sunucu.net", "25565"]);
    // The draft takes the address; what is typed stays as typed.
    rerender(field("mc.sunucu.net", 25565, 0));
    expect(input.value).toBe("https://mc.sunucu.net:25565");

    // Saved.
    rerender(field("mc.sunucu.net", 25565, 1));
    expect(input.value).toBe("mc.sunucu.net:25565");
  });

  it("writes no port back when none was given", () => {
    const { container, rerender } = render(field("", "", 0));
    const input = container.querySelector("input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "http://play.projects.gg/" } });
    rerender(field("play.projects.gg", "", 0));
    expect(input.value).toBe("http://play.projects.gg/");

    rerender(field("play.projects.gg", "", 1));
    expect(input.value).toBe("play.projects.gg");
  });

  it("shows the stored address again when the edits are dropped", () => {
    const { container, rerender } = render(field("play.projects.gg", 0, 0));
    const input = container.querySelector("input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "https://play.projects.gg" } });
    // The same host as stored: the draft does not change, the link stays while typed.
    rerender(field("play.projects.gg", 0, 0));
    expect(input.value).toBe("https://play.projects.gg");

    rerender(field("play.projects.gg", 0, 1));
    expect(input.value).toBe("play.projects.gg");
  });
});
