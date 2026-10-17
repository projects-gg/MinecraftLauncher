import type { LaunchEvent } from "./types";

/**
 * Preview switches that stop a launch with an error, worded as the launcher sends them
 * (`src-tauri/src/error.rs`; the code on the first line of the details, as `tauri.ts` passes it):
 * `?disk=low` (not enough room, before anything is downloaded), `?disk=full` (the disk fills up
 * during the download) and `?launchError=version|process|file|game`. Also from localStorage
 * (`pl.mock.disk`, `pl.mock.launchError`).
 */
function flag(key: string): string {
  try {
    return (
      new URLSearchParams(window.location.search).get(key) ??
      localStorage.getItem(`pl.mock.${key}`) ??
      ""
    );
  } catch {
    return "";
  }
}

/** The launcher keeps a figure and its unit on one line (a no-break space). */
const nb = String.fromCharCode(0xa0);

const stops: Record<string, { code: string; message: string; details: string }> = {
  low: {
    code: "DISK-DOLU",
    message: `Diskte yeterli yer yok. Oyun için 1,2${nb}GB gerekiyor, 300${nb}MB boş.`,
    details:
      "not enough room: 1288490189 bytes needed for 961893990 bytes to download, 314572800 bytes free",
  },
  full: {
    code: "DISK-DOLU",
    message: "Diskte yer kalmadı. Biraz yer açıp tekrar dene.",
    details:
      "I/O error at assets/objects/5f/5ff04807c356f1beed0b86ccf659b44b9983e3fa.part: There is not enough space on the disk. (os error 112)",
  },
  version: {
    code: "SURUM-YOK",
    message: "Bu Minecraft sürümü bulunamadı. Kurulumun sürümünü değiştir.",
    details: "Minecraft version 1.21.9 was not found",
  },
  process: {
    code: "SUREC",
    message: "Oyun başlatılamadı. Tekrar dene.",
    details: "process error: the Java process could not start",
  },
  file: {
    code: "BULUNAMADI",
    message: "Gereken bir dosya bulunamadı. Tekrar dene.",
    details: "client jar of 1.21.1 not found",
  },
  game: {
    code: "OYUN",
    message: "Oyun hazırlanırken bir sorun oluştu. Tekrar dene.",
    details: "invalid data: version 1.21.1 has no mainClass",
  },
};

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Runs the chosen stop: some progress first (the room check comes after the metadata, a full
 * disk part-way through the files), then the error. False when no switch is set.
 */
export async function mockLaunchStop(
  onEvent: (e: LaunchEvent) => void,
  job: { cancelled: boolean },
): Promise<boolean> {
  const disk = flag("disk");
  const key = disk === "low" || disk === "full" ? disk : flag("launchError");
  const stop = stops[key];
  if (!stop) return false;
  const steps = key === "full" ? 14 : 3;
  for (let step = 1; step <= steps; step++) {
    if (job.cancelled) throw new Error("cancelled");
    await wait(110);
    const fraction = step / steps;
    onEvent({
      type: "progress",
      stage: key === "full" ? "assets" : "metadata",
      percent: key === "full" ? 6 + 30 * fraction : 2 * fraction,
      doneFiles: key === "full" ? Math.round(fraction * 1480) : 0,
      totalFiles: key === "full" ? 3204 : 0,
      bytesPerSec: key === "full" ? 11_200_000 : 0,
      etaSecs: key === "full" ? 41 : undefined,
    });
  }
  onEvent({ type: "error", message: stop.message, details: `${stop.code}\n${stop.details}` });
  return true;
}
