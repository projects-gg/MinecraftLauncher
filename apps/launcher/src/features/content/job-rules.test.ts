import { describe, expect, it } from "vitest";
import { bulkJobKey, JOB_REVEAL_MS, projectJobKey, revealWait, updateJobKey } from "./job-rules";

describe("a job's bar", () => {
  it("waits until the job has run 150 ms", () => {
    expect(JOB_REVEAL_MS).toBe(150);
    expect(revealWait(0)).toBe(150);
    expect(revealWait(90)).toBe(60);
    expect(revealWait(150)).toBe(0);
    expect(revealWait(2000)).toBe(0);
  });

  it("shows at once for a job that was running before the list opened", () => {
    // `jobAge` is Infinity for a job that did not start in this list.
    expect(revealWait(Number.POSITIVE_INFINITY)).toBe(0);
  });
});

describe("an update's job", () => {
  const sodium = { projectId: "AANobbMI" };
  const local = { projectId: undefined };

  it("runs one mod from Modrinth under its project's key", () => {
    expect(updateJobKey({}, "i1", "mod", [sodium])).toBe(projectJobKey("i1", "AANobbMI"));
  });

  it("runs several mods, or one without a project, as the kind's update of everything", () => {
    expect(updateJobKey({}, "i1", "mod", [sodium, { projectId: "P7dR8mSH" }])).toBe(
      bulkJobKey("i1", "mod"),
    );
    expect(updateJobKey({}, "i1", "mod", [local])).toBe(bulkJobKey("i1", "mod"));
  });

  it("starts nothing while that instance's update of everything runs", () => {
    const jobs = { [bulkJobKey("i1", "mod")]: { jobId: "j1" } };
    expect(updateJobKey(jobs, "i1", "mod", [sodium])).toBeNull();
    expect(updateJobKey(jobs, "i1", "mod", [sodium, local])).toBeNull();
  });

  it("is not held up by another instance's or another kind's update of everything", () => {
    const jobs = {
      [bulkJobKey("i2", "mod")]: { jobId: "j1" },
      [bulkJobKey("i1", "resourcePack")]: { jobId: "j2" },
    };
    expect(updateJobKey(jobs, "i1", "mod", [sodium])).toBe(projectJobKey("i1", "AANobbMI"));
  });

  it("is not held up by a single mod's own update", () => {
    const jobs = { [projectJobKey("i1", "P7dR8mSH")]: { jobId: "j1" } };
    expect(updateJobKey(jobs, "i1", "mod", [sodium])).toBe(projectJobKey("i1", "AANobbMI"));
  });
});
