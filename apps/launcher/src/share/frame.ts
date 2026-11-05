// "Yayını rapor et" (SHARE-S): what the player sees of a share now, as a JPEG of at most 60 KB
// (base64) that goes with the report as the moderators' evidence. The watch engine's `<video>` is
// drawn on a canvas; the quality steps down (then the size) until it fits. No frame (not watching,
// no picture yet, a canvas that refuses): the report goes without one.
import { pageEngine } from "@/features/share/store";
import { isTauri } from "@/lib/platform";
import { mockFrame } from "./mock-scenarios";

/** The largest still frame the service keeps (bytes). */
export const FRAME_BYTES = 60 * 1024;
const QUALITIES = [0.82, 0.7, 0.58, 0.46, 0.36, 0.28];
const WIDTHS = [1280, 960, 640, 427];
/** How long a stream that is not on screen may take to give its first picture. */
const FIRST_PICTURE_MS = 1500;

/** The bytes a base64 text stands for. */
export function base64Bytes(text: string): number {
  const pad = text.endsWith("==") ? 2 : text.endsWith("=") ? 1 : 0;
  return Math.floor((text.length * 3) / 4) - pad;
}

/**
 * A picture as a JPEG's base64 of at most `limit` bytes: the quality steps down first, then the
 * width; null when even the smallest does not fit or the canvas refuses.
 */
export function jpegOf(
  source: CanvasImageSource,
  width: number,
  height: number,
  limit = FRAME_BYTES,
  canvas: HTMLCanvasElement = document.createElement("canvas"),
): string | null {
  if (width <= 0 || height <= 0) return null;
  const context = canvas.getContext("2d");
  if (!context) return null;
  for (const most of WIDTHS) {
    const scale = Math.min(1, most / width);
    canvas.width = Math.max(1, Math.round(width * scale));
    canvas.height = Math.max(1, Math.round(height * scale));
    context.drawImage(source, 0, 0, canvas.width, canvas.height);
    for (const quality of QUALITIES) {
      const url = canvas.toDataURL("image/jpeg", quality);
      if (!url.startsWith("data:image/jpeg;base64,")) return null;
      const data = url.slice(url.indexOf(",") + 1);
      if (base64Bytes(data) <= limit) return data;
    }
  }
  return null;
}

/** A `<video>` of the stream with a picture: the stage's own, or one made for the moment. */
async function videoOf(
  stream: MediaStream,
): Promise<{ video: HTMLVideoElement; done: () => void }> {
  const shown = Array.from(document.querySelectorAll("video")).find(
    (video) => video.srcObject === stream && video.videoWidth > 0,
  );
  if (shown) return { video: shown, done: () => {} };
  const video = document.createElement("video");
  video.muted = true;
  video.playsInline = true;
  video.srcObject = stream;
  const done = () => {
    video.pause();
    video.srcObject = null;
  };
  await new Promise<void>((resolve) => {
    const timer = window.setTimeout(resolve, FIRST_PICTURE_MS);
    video.addEventListener(
      "loadeddata",
      () => {
        window.clearTimeout(timer);
        resolve();
      },
      { once: true },
    );
    void video.play().catch(() => {});
  });
  return { video, done };
}

/** The preview has no streams: the stand-in picture. */
async function mockPicture(person: string): Promise<string | null> {
  const image = new Image();
  image.src = mockFrame(person);
  await image.decode();
  return jpegOf(image, 1280, 720);
}

/** The player's view of one share now, or null. */
export async function grabShareFrame(person: string, n: number): Promise<string | null> {
  try {
    if (!isTauri()) return await mockPicture(person);
    const stream = pageEngine()?.stream(person, n);
    if (!stream) return null;
    const { video, done } = await videoOf(stream);
    try {
      return jpegOf(video, video.videoWidth, video.videoHeight);
    } finally {
      done();
    }
  } catch {
    return null;
  }
}
