// "Masaüstüne kısayol": a desktop shortcut that starts one instance. Its picture is the
// instance's own icon tile (the chamfered gold-lit tile of the launcher), drawn here at 256 px.
import { toast } from "@pl/ui";
import { api, type InstanceSummary } from "@/lib/api";
import i18n from "@/lib/i18n";
import { instanceIcons } from "@/shell/instance-icon";

const SIZE = 256;
/** The tile's chamfer, scaled from the 7 px cut of a 44 px tile. */
const CUT = Math.round((SIZE * 7) / 44);

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("icon did not load"));
    image.src = src;
  });
}

/** The instance's icon tile as a base64 PNG, or undefined when it cannot be drawn. */
export async function iconTilePng(icon: InstanceSummary["icon"]): Promise<string | undefined> {
  try {
    const image = await loadImage(instanceIcons[icon]);
    const canvas = document.createElement("canvas");
    canvas.width = SIZE;
    canvas.height = SIZE;
    const ctx = canvas.getContext("2d");
    if (!ctx) return undefined;
    ctx.beginPath();
    ctx.moveTo(CUT, 0);
    ctx.lineTo(SIZE, 0);
    ctx.lineTo(SIZE, SIZE - CUT);
    ctx.lineTo(SIZE - CUT, SIZE);
    ctx.lineTo(0, SIZE);
    ctx.lineTo(0, CUT);
    ctx.closePath();
    ctx.clip();
    ctx.fillStyle = "#1b1820";
    ctx.fillRect(0, 0, SIZE, SIZE);
    const glow = ctx.createRadialGradient(
      SIZE / 2,
      SIZE * 0.35,
      0,
      SIZE / 2,
      SIZE * 0.35,
      SIZE * 0.7,
    );
    glow.addColorStop(0, "rgba(232, 154, 44, 0.34)");
    glow.addColorStop(1, "rgba(27, 24, 32, 0)");
    ctx.fillStyle = glow;
    ctx.fillRect(0, 0, SIZE, SIZE);
    const side = SIZE * 0.78;
    ctx.shadowColor = "rgba(10, 9, 12, 0.6)";
    ctx.shadowBlur = 10;
    ctx.shadowOffsetY = 5;
    ctx.drawImage(image, (SIZE - side) / 2, (SIZE - side) / 2, side, side);
    const blob = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/png"));
    if (!blob) return undefined;
    const bytes = new Uint8Array(await blob.arrayBuffer());
    let binary = "";
    for (const byte of bytes) binary += String.fromCharCode(byte);
    return btoa(binary);
  } catch {
    return undefined;
  }
}

/** Create (or refresh) the desktop shortcut of `instance`, with a short confirmation. */
export async function createDesktopShortcut(instance: InstanceSummary) {
  try {
    await api.createShortcut(instance.id, await iconTilePng(instance.icon));
    toast.success(i18n.t("shortcut.created"));
  } catch (error) {
    toast.error(
      error instanceof Error && error.message ? error.message : i18n.t("shortcut.failed"),
    );
  }
}
