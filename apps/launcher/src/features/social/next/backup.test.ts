import { describe, expect, it } from "vitest";
import { buildBackup, readBackup } from "../model";
import { cleanIgnored, cleanPinned } from "./prefs";

const friends = {
  favorites: [],
  notifyFavoriteOnline: true,
  notifyFavoriteProjects: true,
};

describe("SOC-NEXT in the settings backup", () => {
  it("carries who is ignored and the pinned conversations, ids only", () => {
    const document = buildBackup(
      {},
      {
        ...friends,
        ignored: ["101", "101", "not-an-id"],
        pinnedChats: ["d:100:101", "c:0123456789abcdef", "x:1", "d:100:102", "d:100:103"],
      },
    );
    expect(document.friends.ignored).toEqual(["101"]);
    expect(document.friends.pinnedChats).toEqual(["d:100:101", "c:0123456789abcdef", "d:100:102"]);
    const read = readBackup(JSON.parse(JSON.stringify(document)));
    expect(read.friends.ignored).toEqual(["101"]);
    expect(read.friends.pinnedChats).toHaveLength(3);
  });

  it("leaves them out while empty (older backups stay as they were)", () => {
    const document = buildBackup({}, { ...friends, ignored: [], pinnedChats: [] });
    expect("ignored" in document.friends).toBe(false);
    expect("pinnedChats" in document.friends).toBe(false);
    expect(readBackup({ friends: {} }).friends.ignored).toBeUndefined();
  });

  it("never carries nicknames", () => {
    const document = buildBackup({}, friends);
    expect(JSON.stringify(document)).not.toContain("nickname");
  });

  it("bounds what a backup may bring", () => {
    expect(cleanIgnored(Array.from({ length: 300 }, (_, i) => String(i)))).toHaveLength(200);
    expect(cleanIgnored("101")).toEqual([]);
    expect(cleanPinned(["d:1:2", "d:1:2"])).toEqual(["d:1:2"]);
  });
});
