# @pl/stickers

The Projects chat stickers. A message carries only a reference such as `pl1/gg`; every client draws
the sticker from the files that come with it, so a set changes only with an update (the launcher, the
Android app, the in-game overlay mod and the friends service's allow list all read this package).

## Set `pl1`

32 stickers of "Küp", a small obsidian cube with gold edges and pixel eyes, in four themes of eight:
Tepkiler / Reactions, Oyun / Game, Sunucu / Server, Selamlaşma / Greetings.

| Path | What | Used by |
| --- | --- | --- |
| `pl1/manifest.json` | ids, references, themes, Turkish and English names and keywords, captions, motion, files with bytes and SHA-256, atlas cells | everyone |
| `pl1/launcher/<id>.webp` | 256 px WebP (quality 85, lossless alpha), shown at 112 px in chat and 64 px in the picker | launcher, Studio |
| `pl1/android/pl1_<id>.webp` | 384 px WebP (120 dp at xxhdpi) | Android app (copied by its build) |
| `pl1/overlay/pl1-atlas.png` | one 1280 × 640 palette PNG, 8 × 4 tiles of 160 px, row = theme, column = order in the theme | overlay mod |
| `pl1/SHA256SUMS.txt` | checksums of the images | tests, copies |

`src/index.ts` reads and checks a manifest (`parseStickerSet`, `findSticker`, `isKnownSticker`, the
reference pattern the service accepts). `src/images.ts` gives the launcher's image URLs and needs a
bundler (Vite). The tests (`pnpm --filter @pl/stickers test`) check every file against the manifest
and the size budget.

### Motion

Stickers are still images. Each one moves once, in code (transform and opacity only), when it arrives
and when the pointer comes onto it or it is pressed; nothing loops, and with reduced motion it only
fades in. `motion` names the pattern and `motions` says in words what each pattern does, so every
client moves a sticker the same way. The patterns were chosen with the set (owner, 22 Oct 2026):
`lol` wobbles, `love` beats, `boom` shakes, `hi` waves, `party` bounces.

### Source and licence

The art is Projects' own, generated with AI assistance (Codex `image_gen`) from one mascot sheet and
post-processed by script: transparent ground, an even white die-cut edge and an ember shadow drawn by
the script, the caption plates set in Tektur 600 (SIL Open Font License 1.1). No Mojang characters,
logos or textures. The exports here were resized from the 1024 px masters (Lanczos with premultiplied
alpha; WebP method 6; the atlas quantized to 256 colours with libimagequant). The masters (~19 MB)
and the generation and post-processing tools stay in the project's art archive, outside this
repository; `motion` and `motions` were added to the manifest here.

Projects owns the art and licenses it with each repository's own licence: here, as the rest of this
repository, GPL-3.0-only (`LICENSE` at the root), also in the overlay mod that ships the atlas; in the
Android app, which copies the files, its LGPL.
