// Browser preview of `api.data`: a pretend data folder with live-looking numbers.
// `localStorage["pl.mock.pickFolder"]` overrides the folder the picker returns (screenshots):
// a name with "Dolu" fails like a full folder, "Küçük" lacks space.
// `localStorage["pl.mock.unreachable"]` pretends a moved folder on that path is missing.
import type { DataApi, DataOverview, MoveProgress } from "./data-types";
import { pick } from "./mock-lang";

const delay = (ms: number) => new Promise((r) => setTimeout(r, ms));
const MB = 1024 * 1024;
const GB = 1024 * MB;

/** Same shape as the app's command errors, so the UI shows the friendly message. */
class MockDataError extends Error {
  readonly details = "mock";
  constructor(
    readonly code: string,
    message: string,
  ) {
    super(message);
  }
}

/** The move was cancelled, worded like the backend (`move_error`). */
const moveCancelled = () =>
  new MockDataError(
    "IPTAL",
    pick(
      "Taşıma iptal edildi. Verilerin eski yerinde duruyor.",
      "Move cancelled. Your data is still in its old place.",
    ),
  );

export function createMockData(): DataApi {
  const state: DataOverview = {
    root: "C:\\Users\\oyuncu\\AppData\\Roaming\\ProjectsLauncher",
    cacheBytes: Math.round(486.4 * MB),
    logsBytes: Math.round(23.7 * MB),
    freeBytes: Math.round(118.2 * GB),
    movable: true,
  };
  let cancelled = false;

  return {
    async overview() {
      await delay(450);
      return { ...state, unreachable: localStorage.getItem("pl.mock.unreachable") ?? undefined };
    },
    async openFolder() {},
    async createSupportBundle() {
      await delay(1600);
      const now = new Date();
      const two = (n: number) => String(n).padStart(2, "0");
      const fileName = `projects-destek-${now.getFullYear()}-${two(now.getMonth() + 1)}-${two(
        now.getDate(),
      )}-${two(now.getHours())}${two(now.getMinutes())}.zip`;
      return { path: `C:\\Users\\oyuncu\\Downloads\\${fileName}`, fileName, folder: "downloads" };
    },
    async revealFile() {},
    async pickFolder() {
      await delay(300);
      return localStorage.getItem("pl.mock.pickFolder") ?? "D:\\Oyunlar";
    },
    async planMove(picked) {
      await delay(900);
      if (picked.includes("Dolu")) {
        throw new MockDataError(
          "TASI-DOLU",
          pick(
            "Seçtiğin yerde içi dolu bir ProjectsLauncher klasörü var. Başka bir klasör seç.",
            "There's already a ProjectsLauncher folder with files in it there. Pick another folder.",
          ),
        );
      }
      const small = picked.includes("Küçük");
      const totalBytes = Math.round(6.8 * GB);
      const freeBytes = small ? Math.round(3.1 * GB) : Math.round(412.6 * GB);
      return {
        from: state.root,
        target: `${picked.replace(/[\\/]+$/, "")}\\ProjectsLauncher`,
        totalBytes,
        fileCount: 18342,
        freeBytes,
        enoughSpace: freeBytes > totalBytes + 512 * MB,
      };
    },
    async move(_picked, onProgress) {
      cancelled = false;
      const totalBytes = Math.round(6.8 * GB);
      const totalFiles = 18342;
      const send = (p: Partial<MoveProgress>) =>
        onProgress({ stage: "copying", doneBytes: 0, totalBytes, doneFiles: 0, totalFiles, ...p });
      send({ stage: "scanning", totalBytes: 0, totalFiles: 0 });
      await delay(700);
      const steps = 60;
      for (let i = 1; i <= steps; i++) {
        if (cancelled) throw moveCancelled();
        await delay(110);
        const share = i / steps;
        send({
          doneBytes: Math.round(totalBytes * share),
          doneFiles: Math.round(totalFiles * share),
        });
      }
      send({ stage: "verifying", doneBytes: totalBytes, doneFiles: totalFiles });
      await delay(900);
      if (cancelled) throw moveCancelled();
    },
    async cancelMove() {
      cancelled = true;
    },
  };
}
