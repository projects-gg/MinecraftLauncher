# Content format (`v1`)

Normative description of the content tree that Projects Studio publishes and the launcher (and
later the Android app) consumes. Implemented by `crates/pl-content`; the JSON examples below are
checked by its tests (`doc_examples`), so they must stay valid.

## Tree

Base URLs (mirrors, tried in this order, then the local cache):

- `https://cdn.projects.gg/v1/` — Cloudflare R2 (primary)
- `https://raw.githubusercontent.com/projects-gg/launcher-content/main/v1/` — GitHub mirror

```text
v1/index.json                      signed root (the only mutable document besides channels)
v1/index.json.sig                  { keyId, signature } over the exact bytes of index.json
v1/config/launcher.<sha256>.json   LauncherConfig (home, banners, servers, links, RPC, flags)
v1/news/news.<sha256>.json         NewsFeed
v1/media/<sha256>.<ext>            images and videos
v1/packs/<id>/<version>.json       PackManifest (.mrpack index + "projects" extension)
v1/files/<sha1>                    files we host for packs (own mods, configs)
v1/afk/<version>/<platform>.zip    AFK client builds
v1/releases/<channel>.json         launcher update channel (stable, beta)
v1/releases/<version>/…            installers / updater bundles and their .sig files
```

Everything except `index.json`, `index.json.sig` and `releases/<channel>.json` is immutable:
a path never changes content. `<sha256>` / `<sha1>` are the lowercase hex digest of the file
itself; pack manifests are immutable per version (a change needs a new `versionId`).

## Conventions

- UTF-8 JSON, `camelCase` keys (except `.mrpack` and Tauri fields that have their own names).
  Studio writes canonical JSON: keys sorted, 2-space indent, `\n`, trailing newline.
- Timestamps: RFC 3339 in UTC (`2026-10-15T12:00:00Z`). Hashes: lowercase hex.
- Paths inside documents are `/`-separated and relative to the mirror base. Mirror paths use
  only `[A-Za-z0-9._-/]`.
- Forward compatibility: readers ignore unknown fields; every optional field has a default;
  unknown enum values map to a catch-all (`unknown`, or the safest value where noted).
- `schema` (root, config, news) is a major version, bumped only for incompatible changes;
  readers refuse documents whose schema is higher than they support. Additive changes never
  bump it. An incompatible redesign gets a new tree (`v2/`).

## Root index and signature

<!-- example: RootIndex -->
```json
{
  "schema": 1,
  "sequence": 1792065600,
  "generatedAt": "2026-10-15T12:00:00Z",
  "config": {
    "path": "config/launcher.b79606fb3afea5bd1609ed40b622142f1c98125abcfe89a76a661b0e8e343910.json",
    "sha256": "b79606fb3afea5bd1609ed40b622142f1c98125abcfe89a76a661b0e8e343910",
    "size": 4182
  },
  "news": {
    "path": "news/news.19fba0e995b9794fc2c26217bf3b725c2f0d9eeda16719fe75e3ba23ca73bfc4.json",
    "sha256": "19fba0e995b9794fc2c26217bf3b725c2f0d9eeda16719fe75e3ba23ca73bfc4",
    "size": 9120
  },
  "packs": [
    {
      "id": "projects",
      "name": "Projects",
      "latest": "2.3.0",
      "manifest": {
        "path": "packs/projects/2.3.0.json",
        "sha256": "4862f447f2c7f272fa2f4aaf89dadb3b1ac09105bd5864f8d1a0c9452bb0a226",
        "size": 18342
      }
    }
  ],
  "afk": {
    "version": "1.4.0",
    "builds": {
      "windows-x86_64": {
        "path": "afk/1.4.0/windows-x86_64.zip",
        "urls": ["https://github.com/projects-gg/Projects-Console-Client/releases/download/1.4.0/windows-x86_64.zip"],
        "sha256": "fb960cdfed351d8cf365d8d6afd56d042ac54c1bd9a1dd0b139691c5c3d074e0",
        "size": 31457280
      },
      "linux-x86_64": {
        "path": "afk/1.4.0/linux-x86_64.zip",
        "sha256": "75b991aa166bcad853a8bea71cffea48c4b72a4c84379cfda24b01f88aaddce3",
        "size": 30408704
      }
    }
  }
}
```

- `DocRef { path, sha256, size }` points at a JSON document; clients verify size and SHA-256.
- `FileRef { path?, urls?, sha256, sha1?, size }` points at a binary: `path` is tried on every
  mirror first, then the absolute `urls` (e.g. GitHub release assets for large files).
- AFK platform keys: `windows-x86_64`, `linux-x86_64`, `linux-aarch64`, `macos-aarch64`,
  `macos-x86_64` (`<os>-<arch>` as Rust spells them).

<!-- example: SignatureFile -->
```json
{
  "keyId": "3f0c2a91d84b6e57",
  "signature": "f3/2+aJAWiRGZyyMGsRT+gslebg8DMK/vkEkKJ+NXHP4PuMIfPtcwcNUhTkZWwXyjxl/oW+htiePm++M/lFdMg=="
}
```

- `signature`: Ed25519 over the exact bytes of `index.json`, standard base64 with padding.
- `keyId`: lowercase hex of the first 8 bytes of SHA-256(32-byte public key).
- Keys are exchanged as base64 of their 32 raw bytes (no PEM). The secret key lives only in
  Studio's OS keychain (`gg.projects.studio`); the launcher embeds a list of trusted public keys.
- Rotation: ship a launcher that trusts old + new, wait for adoption, sign with the new key,
  later drop the old key from the list.

Client acceptance rules for an index fetched from a mirror:

1. `keyId` must match a trusted key and the signature must verify (strict Ed25519). On a
   mismatch the client refetches `index.json` and `.sig` once with `?t=<ms>` to get past edge
   caches (mirrors must ignore unknown query strings), then moves to the next mirror.
2. `schema` must be supported; every `DocRef` must be well formed.
3. Rollback protection: `sequence` must be greater than the cached index's sequence. An equal
   sequence is accepted only when the bytes are identical (same publish from another mirror).
4. `config` and `news` are downloaded (preferring the same mirror) and verified before the new
   index replaces the cached one.

Studio sets `sequence = max(previous + 1, unix seconds of generatedAt)`, so it keeps growing
even if Studio lost track of the previous publish.

## Media

<!-- example: MediaRef -->
```json
{
  "path": "media/4c5ed0886b06a3f90744efa271b0a15709d8cb400993566fd2fe3f8c68ca6d5a.webm",
  "sha256": "4c5ed0886b06a3f90744efa271b0a15709d8cb400993566fd2fe3f8c68ca6d5a",
  "size": 4194304,
  "kind": "video",
  "width": 1920,
  "height": 1080,
  "poster": {
    "path": "media/c48ca766628ec6758c30941cc83e524c5cd4eff4f51bf5166862b7c3077bfa14.webp",
    "sha256": "c48ca766628ec6758c30941cc83e524c5cd4eff4f51bf5166862b7c3077bfa14",
    "size": 182044,
    "kind": "image",
    "width": 1920,
    "height": 1080,
    "blurhash": "LKO2?U%2Tw=w]~RBVZRi};RPxuwH"
  },
  "alt": "Altın ejderha, kor parçacıkları"
}
```

`kind` is `image` or `video` (unknown kinds: show the poster). Images are WebP, videos WebM VP9
with a WebP poster (used in performance mode and on Linux). The extension in `path` is the
cache file extension.

## Launcher config

<!-- example: LauncherConfig -->
```json
{
  "schema": 1,
  "minLauncherVersion": "2.0.0",
  "maintenance": {
    "enabled": false,
    "title": "Bakım çalışması",
    "message": "Sunucular kısa bir süre bakımda olacak.",
    "until": "2026-10-15T14:00:00Z"
  },
  "banners": [
    {
      "id": "winter-2026",
      "level": "event",
      "text": "Kış etkinliği başladı!",
      "url": "https://projects.gg/kis",
      "startsAt": "2026-12-01T00:00:00Z",
      "endsAt": "2027-01-07T00:00:00Z",
      "dismissible": true
    }
  ],
  "hero": {
    "media": {
      "path": "media/c48ca766628ec6758c30941cc83e524c5cd4eff4f51bf5166862b7c3077bfa14.webp",
      "sha256": "c48ca766628ec6758c30941cc83e524c5cd4eff4f51bf5166862b7c3077bfa14",
      "size": 182044,
      "kind": "image",
      "blurhash": "LKO2?U%2Tw=w]~RBVZRi};RPxuwH"
    },
    "title": "Yeni sezon başladı",
    "subtitle": "Kış haritası ve yeni görevler",
    "cta": { "label": "Yama notları", "newsId": "patch-2-3" },
    "particles": "embers",
    "accent": "#E8B040"
  },
  "theme": { "season": "winter", "accent": "#7FB2E5" },
  "events": [
    {
      "id": "bedwars-cup",
      "title": "Bed Wars turnuvası",
      "startsAt": "2026-10-20T18:00:00Z",
      "endsAt": "2026-10-20T21:00:00Z",
      "url": "https://projects.gg/turnuva"
    }
  ],
  "servers": [
    {
      "id": "main",
      "name": "Projects",
      "address": "play.projects.gg",
      "port": 25565,
      "featured": true,
      "description": "Ana sunucu",
      "icon": {
        "path": "media/c2d4b446a44ce54fab8e01150e24dd24f3d850c7c14dcfe31f6321341dd86874.png",
        "sha256": "c2d4b446a44ce54fab8e01150e24dd24f3d850c7c14dcfe31f6321341dd86874",
        "size": 4096,
        "kind": "image",
        "width": 64,
        "height": 64
      }
    }
  ],
  "links": {
    "website": "https://projects.gg",
    "discord": "https://discord.gg/projects",
    "store": "https://store.projects.gg",
    "support": "https://projects.gg/destek",
    "custom": [
      { "id": "youtube", "label": "YouTube", "url": "https://youtube.com/@projectsgg", "icon": "youtube" }
    ]
  },
  "discord": {
    "enabled": true,
    "clientId": "949311557542756362",
    "templates": {
      "idle": { "details": "Launcher'da", "largeImage": "logo", "largeText": "Projects Launcher" },
      "downloading": { "details": "{instance} indiriliyor", "state": "%{percent}" },
      "inGame": {
        "details": "{instance} · {version}",
        "state": "{server}",
        "largeImage": "logo",
        "smallImage": "https://mc-heads.net/avatar/{player}/64",
        "smallText": "{player}",
        "buttons": [{ "label": "Sunucuya katıl", "url": "https://projects.gg/oyna" }]
      },
      "afk": { "details": "{afkCount} AFK hesap" }
    }
  },
  "features": { "afk": true, "premiumLogin": true, "offlineAccounts": true, "modrinthBrowser": true },
  "recommendedPack": "projects"
}
```

Every field is optional (`{}` is a valid config). Defaults: no maintenance, no banners, bundled
hero art with `particles: embers`, `season: none`, all feature flags `true`, Discord enabled
with the launcher's built-in client id when `clientId` is empty.

- `banners[].level`: `info | warning | event | critical`; a banner is shown while
  `startsAt <= now < endsAt` (missing bounds are open). `dismissible` defaults to `true`.
- `hero.particles`: `embers | sparks | snow | none`. `theme.season`:
  `none | halloween | winter | holiday | event`. Unknown values fall back to the defaults.
- `cta`: `{ label, url? , newsId? }` — `newsId` opens an article, `url` opens the browser.
- `events[]`: countdown until `startsAt`, "live" until `endsAt`, hidden afterwards.
- `maintenance` is active while `enabled` and (`until` missing or `now < until`).
- `minLauncherVersion`: older launchers show the lock screen and install the update.
- Discord templates exist for `idle`, `browsing`, `downloading`, `inGame`, `afk`. Text fields
  accept `{instance}`, `{version}`, `{loader}`, `{server}`, `{players}`, `{maxPlayers}`,
  `{percent}`, `{afkCount}`, `{player}`; unknown placeholders render empty, empty fields are
  dropped. Images are asset keys or `https://` URLs; at most two buttons.

## News

<!-- example: NewsFeed -->
```json
{
  "schema": 1,
  "items": [
    {
      "id": "patch-2-3",
      "title": "2.3 yama notları",
      "summary": "Yeni modlar ve performans iyileştirmeleri.",
      "body": "## Yenilikler\n\n- Sodium 0.6\n- Daha hızlı açılış",
      "cover": {
        "path": "media/3fa405a8301ace34d11cf44a816080b8f0e49a48fbd048b8aef1543a8c58bdb6.webp",
        "sha256": "3fa405a8301ace34d11cf44a816080b8f0e49a48fbd048b8aef1543a8c58bdb6",
        "size": 98304,
        "kind": "image",
        "width": 1280,
        "height": 720,
        "blurhash": "L6PZfSi_.AyE_3t7t7R**0o#DgR4"
      },
      "category": "patchNotes",
      "tags": ["güncelleme", "modlar"],
      "publishedAt": "2026-10-15T09:00:00Z",
      "expiresAt": "2026-12-31T23:59:59Z",
      "pinned": true,
      "featured": true,
      "cta": { "label": "Paketi güncelle", "url": "projects://pack/projects" },
      "author": "Projects Ekibi"
    }
  ]
}
```

- `category`: `update | event | announcement | patchNotes | community` (unknown → like
  `announcement`). `body` is Markdown, rendered sanitized; links open in the system browser.
- Visible when `publishedAt <= now` and (`expiresAt` missing or `now < expiresAt`), so posts
  can be scheduled. Order: pinned first, then newest `publishedAt`, then `id`.
- `featured` items (and the hero) are prefetched for offline display.

## Modpacks

A pack manifest is a Modrinth `.mrpack` index (`modrinth.index.json`, `formatVersion: 1`) plus
a top-level `projects` object that other launchers ignore; Studio can export a real `.mrpack`.

<!-- example: PackManifest -->
```json
{
  "formatVersion": 1,
  "game": "minecraft",
  "versionId": "2.3.0",
  "name": "Projects",
  "summary": "Resmî Projects mod paketi",
  "files": [
    {
      "path": "mods/sodium-fabric-0.6.0+mc1.21.1.jar",
      "hashes": {
        "sha1": "6f07dc1d0a287f925d7672e6884d85ceb3f339ed",
        "sha512": "961b4af816284b41547aaff17bef5eae144f4a29dafa7b1819100e0e8deb93bea53946b38c579ba6a74162f3bdbc5adc672d4ec52425971d3072c37e18430fc0"
      },
      "env": { "client": "required", "server": "unsupported" },
      "downloads": ["https://cdn.modrinth.com/data/AANobbMI/versions/u1OEbNKx/sodium-fabric-0.6.0%2Bmc1.21.1.jar"],
      "fileSize": 1048576
    },
    {
      "path": "options.txt",
      "hashes": {
        "sha1": "513f8de9259fe7658fe14d1352c54ccf070e911f",
        "sha512": "434f88686f2ea6e9275c85fd552a1695baf58a6813ac6be1fb2e5099e1a77e90d7e0958586bd2ffc9dcfbe8cc23a1c02138bbebbd696ab6bdcad772f958dd052"
      },
      "downloads": [
        "https://cdn.projects.gg/v1/files/513f8de9259fe7658fe14d1352c54ccf070e911f",
        "https://raw.githubusercontent.com/projects-gg/launcher-content/main/v1/files/513f8de9259fe7658fe14d1352c54ccf070e911f"
      ],
      "fileSize": 2048
    },
    {
      "path": "shaderpacks/ComplementaryReimagined_r5.4.zip",
      "hashes": {
        "sha1": "68477749788526fe4e350828be11ee864ef26548",
        "sha512": "0242486e42dc14d08c974057aca55ade8f19c6994fdb66c19badff7be787f8ebffcf9a5d3684318556479a7faaf89761ca25042fca6c6f7282fbb5c76b612ba6"
      },
      "env": { "client": "optional", "server": "unsupported" },
      "downloads": ["https://cdn.modrinth.com/data/HVnmMxH1/versions/xT4e9w1L/ComplementaryReimagined_r5.4.zip"],
      "fileSize": 7340032
    }
  ],
  "dependencies": { "minecraft": "1.21.1", "fabric-loader": "0.16.9" },
  "projects": {
    "packId": "projects",
    "changelog": "## 2.3.0\n\n- Sodium 0.6\n- Yeni shader paketi (isteğe bağlı)",
    "optionalGroups": [
      {
        "id": "shaders",
        "name": "Shader paketi",
        "description": "Güçlü ekran kartları için gölgelendiriciler.",
        "defaultEnabled": false,
        "files": ["shaderpacks/ComplementaryReimagined_r5.4.zip"]
      }
    ],
    "configPolicy": {
      "default": "overwrite",
      "rules": [
        { "glob": "options.txt", "policy": "ifMissing" },
        { "glob": "config/**", "policy": "preserveUserChanges" }
      ]
    },
    "servers": [{ "name": "Projects", "address": "play.projects.gg" }],
    "recommendedMemoryMb": 4096,
    "minLauncherVersion": "2.0.0"
  }
}
```

- `dependencies`: `minecraft` plus at most one of `fabric-loader`, `quilt-loader`, `forge`,
  `neoforge` (mrpack spelling).
- `files[].downloads`: absolute HTTP(S) URLs tried in order. Files we host are uploaded to
  `files/<sha1>` and listed with both the R2 and the GitHub URL. Clients verify SHA-512 when
  present, otherwise SHA-1, and the size.
- `env.client`: `required` (default) | `optional` | `unsupported` (server-only, never installed
  on clients). Optional files that are not in a group are installed.
- `optionalGroups`: switchable sets of `files[]` paths. A file in several groups is installed
  when any of them is enabled. The player's choice is remembered in the pack state; new groups
  start at `defaultEnabled`.
- `configPolicy`: `rules` are checked top to bottom, first match wins, otherwise `default`.
  Policies: `overwrite` (always match the pack), `ifMissing` (install only when absent),
  `preserveUserChanges` (update only while unchanged since we installed it). Unknown policies
  behave like `preserveUserChanges`.
- Glob syntax (case-sensitive, whole path): `*` = any characters except `/`, `**` = any
  characters including `/` (`**/` also matches zero folders), `?` = one character except `/`.
  `*.txt` matches only top-level files; use `**/*.txt` for any depth.

### Path rules

`files[].path` is relative to the game directory and is rejected (the whole manifest fails)
when it is empty or longer than 512 bytes, starts with `/`, contains `\`, `:`, control
characters or `<>"|?*`, has empty, `.` or `..` components, has a component ending in `.` or
space, uses a Windows device name (`CON`, `PRN`, `AUX`, `NUL`, `COM0-9`, `LPT0-9`, also with an
extension), or starts with `.projects/`. Duplicate paths (case-insensitive) and a path that is
both a file and a folder are rejected too.

### Sync semantics and pack state

The launcher records what it installed in `<game dir>/.projects/pack-state.json`:

<!-- example: PackState -->
```json
{
  "packId": "projects",
  "versionId": "2.3.0",
  "files": {
    "mods/sodium-fabric-0.6.0+mc1.21.1.jar": { "sha1": "6f07dc1d0a287f925d7672e6884d85ceb3f339ed", "policy": "overwrite" },
    "options.txt": { "sha1": "513f8de9259fe7658fe14d1352c54ccf070e911f", "policy": "ifMissing" }
  },
  "groups": { "shaders": false }
}
```

For every file the client should have (not `unsupported`, not in a disabled group):

| On disk | Policy | Action |
| --- | --- | --- |
| missing, `<path>.disabled` exists | any | keep disabled |
| missing | any | download |
| same SHA-1 as the manifest | any | unchanged |
| different | `overwrite` | download (replace) |
| different | `ifMissing` | keep |
| different, equals the SHA-1 we recorded | `preserveUserChanges` | download (update) |
| different, changed by the player or never ours | `preserveUserChanges` | keep |

Files recorded in the state but no longer wanted (removed from the manifest, group disabled,
now `unsupported`) are deleted only while their SHA-1 still equals the recorded one (also as
`<path>.disabled`); changed files are kept. Files that never appeared in the state (mods the
player added) are never touched. "Repair" treats every policy as `overwrite`. Downloads happen
first (verified, atomic); deletions and the new state follow only when all downloads
succeeded.

## Launcher release channels

`releases/<channel>.json` is Tauri's static updater JSON plus fields Tauri ignores:

<!-- example: ReleaseChannel -->
```json
{
  "version": "2.1.0",
  "notes": "Yeni ana ekran ve daha hızlı indirmeler.",
  "pub_date": "2026-10-15T12:00:00Z",
  "platforms": {
    "windows-x86_64": {
      "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkK",
      "url": "https://cdn.projects.gg/v1/releases/2.1.0/Projects.Launcher_2.1.0_x64-setup.exe"
    },
    "darwin-aarch64": {
      "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkK",
      "url": "https://cdn.projects.gg/v1/releases/2.1.0/Projects.Launcher_aarch64.app.tar.gz"
    },
    "linux-x86_64": {
      "signature": "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkK",
      "url": "https://cdn.projects.gg/v1/releases/2.1.0/projects-launcher_2.1.0_amd64.AppImage"
    }
  },
  "rollout": 25,
  "mandatory": false,
  "minVersion": "2.0.3",
  "channel": "stable"
}
```

- `platforms` keys are Tauri targets (`windows-x86_64`, `darwin-aarch64`, `darwin-x86_64`,
  `linux-x86_64`, …); `signature` is the content of the bundle's minisign `.sig` file.
- `rollout` (0–100, default 100): an install is offered the release when
  `bucket < rollout`, with `bucket = u32_be(first 4 bytes of SHA-256(install id)) % 100`.
- `mandatory`: eligible installs must update before playing.
- `minVersion`: installs older than this always get a mandatory update, regardless of rollout.
- The launcher fetches the channel file from R2 first, then the GitHub mirror.

## HTTP caching

| Objects | `Cache-Control` |
| --- | --- |
| `index.json`, `index.json.sig`, `releases/<channel>.json` | `public, max-age=60` |
| `config/*`, `news/*`, `media/*`, `files/*`, `packs/*/*.json`, `afk/*/*`, `releases/<version>/*` | `public, max-age=31536000, immutable` |

JSON is served as `application/json; charset=utf-8`. Mirrors must return an `ETag` for
`index.json` and honour `If-None-Match` (clients send it and treat `304` as "unchanged").
GitHub's raw mirror uses its own headers (about 5 minutes), which only delays updates while the
primary is down.

## Publish order

1. Upload new immutable files: media, `files/<sha1>`, AFK builds, release bundles.
2. Upload the documents: `config/…`, `news/…`, `packs/<id>/<version>.json`.
3. Check that every new object is readable with the expected size and hash.
4. Upload `index.json.sig`, then `index.json` last (the commit point). For updates:
   `releases/<version>/…` first, `releases/<channel>.json` last.
5. Mirror to GitHub in the same order (immutable files and large release assets first, then
   one commit with `index.json` + `.sig`).
6. Keep immutable files referenced by the previous indexes for a while; clients that have not
   refreshed yet still use them. Clean up later.

## Client cache

The launcher keeps the last verified snapshot in `<data>/cache/content/`:

```text
current.json            exact index.json text + signature, ETags per mirror, fetch time
docs/<sha256>.json      config, news, pack manifests (verified on every read)
media/<sha256>.<ext>    media (verified on download, then trusted by size)
```

Start-up shows the cached snapshot immediately, then refreshes (short timeout). If every mirror
fails, the cached snapshot stays in use with an offline flag; content is never shown unless its
signature verifies against a trusted key. After a refresh that brings a new index, documents and
media referenced by neither the current nor the previous snapshot are deleted.
