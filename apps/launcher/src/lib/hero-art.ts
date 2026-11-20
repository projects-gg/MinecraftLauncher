import hero1672 from "@/assets/art/hero-dragon-1.webp";
import hero2240 from "@/assets/art/hero-dragon-2240.webp";
import hero2560 from "@/assets/art/hero-dragon-2560.webp";

/**
 * The bundled key art and its larger copies (HERO-SHARP). The master is only 1672x941
 * (`D:/ProLauncher-build/art/hero-dragon-1.png`; lossless copy in LandingPage
 * `assets/art/hero-dragon-master.webp`), and Home draws it over the window plus 48 px, zoomed up to
 * 1.13 by the drift: a maximised 1920x1080 window stretched it ~1.36x, 2560x1440 or Windows scaling
 * of 125-150 % up to ~2.3x, and it looked soft. All three are WebP q88 (every webview decodes it;
 * AVIF is not certain on WebKitGTK or older macOS) from the lossless master; 2240 and 2560 are
 * Lanczos upscales with a light sharpen (sigma 0.6): resampling, not new detail, but sharper than
 * the webview's own scaling. Real 4K needs a larger master.
 */
export const heroArt = hero1672;

const HERO_SRCSET = `${hero1672} 1672w, ${hero2240} 2240w, ${hero2560} 2560w`;

/** The larger copies, only for the bundled art (art from Studio is a single file). */
export function heroArtSrcSet(url: string): string | undefined {
  return url === hero1672 ? HERO_SRCSET : undefined;
}
