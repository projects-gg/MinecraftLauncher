# Sign-in result page assets

Embedded (as `data:` URIs) in the page the browser shows after Microsoft redirects back to the
launcher's loopback (`src/page.rs`). They are the same files the friends service's browser pages
use, so both sign-in pages look alike.

| File | Source | Licence |
| --- | --- | --- |
| `art-1280.webp` | The season's key art (`apps/launcher/src/assets/art/hero-dragon-1.webp`, the landing page's `hero-dragon`), lightly blurred, 1280 px wide | projects.gg artwork |
| `wordmark-320.webp` | The PROJECTS.GG wordmark (landing page `assets/brand/wordmark.png`), 320 px wide | projects.gg brand |
| `icon-32.png` | The app icon (`assets/brand/app-icon-1024.png`), 32 px | projects.gg brand |
| `tektur-latin.woff2`, `figtree-latin.woff2` | The launcher's latin builds (`packages/ui/src/fonts/`) | SIL OFL 1.1 (`packages/ui/src/fonts/OFL-*.txt`) |
| `tektur-tr.woff2`, `figtree-tr.woff2` | fontsource 5.3.0 latin-ext, cut down to the Turkish letters it adds (Ğ ğ İ Ş ş) | SIL OFL 1.1 |

The page loads all of them inline (about 95 KB of HTML); it is served once per sign-in on
localhost, so size is not a concern. Replace a file in place and keep its name; `ART_SIZE` in
`src/page.rs` holds the art's pixel size.
