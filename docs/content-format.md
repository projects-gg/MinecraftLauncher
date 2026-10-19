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
v1/modsets/modsets.<sha256>.json   ModSetCatalog (ready-made mod sets, optional)
v1/files/<sha1>                    files we host for packs (own mods, configs)
v1/afk/<version>/<platform>.zip    AFK client builds
v1/releases/<channel>.json         launcher update channel (stable, beta)
v1/releases/<version>/…            installers / updater bundles and their .sig files
```

Everything except `index.json`, `index.json.sig` and `releases/<channel>.json` is immutable:
a path never changes content. `<sha256>` / `<sha1>` are the lowercase hex digest of the file
itself; pack manifests are immutable per version (a change needs a new `versionId`).

The Discord bot's news feed is a separate tree on its own mirrors, enabled and trusted through
the launcher config: see [News feed (Discord bot)](#news-feed-discord-bot).

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
      "minecraft": "1.21.1",
      "default": true,
      "manifest": {
        "path": "packs/projects/2.3.0.json",
        "sha256": "4862f447f2c7f272fa2f4aaf89dadb3b1ac09105bd5864f8d1a0c9452bb0a226",
        "size": 18342
      }
    },
    {
      "id": "projects-1.21.4",
      "name": "Projects",
      "latest": "1.0.0",
      "family": "projects",
      "minecraft": "1.21.4",
      "manifest": {
        "path": "packs/projects-1.21.4/1.0.0.json",
        "sha256": "35e6d9639fe62cac51ee980905992c67ff7642116a2a5b1915222b7c976fbbaf",
        "size": 9120
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
    },
    "minecraftVersions": [
      { "value": "1.21.9", "label": "1.21.9 – 1.21.10" },
      { "value": "1.21.5", "label": "1.21.5 – 1.21.8" },
      { "value": "1.21.4", "label": "1.21.4" }
    ],
    "recommendedMinecraftVersion": "1.21.9"
  },
  "modSets": {
    "path": "modsets/modsets.6d1f0c2a9b8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4a392817061f2e.json",
    "sha256": "6d1f0c2a9b8e7d6c5b4a39281706f5e4d3c2b1a09f8e7d6c5b4a392817061f2e",
    "size": 6214
  }
}
```

- `DocRef { path, sha256, size }` points at a JSON document; clients verify size and SHA-256.
- `FileRef { path?, urls?, sha256, sha1?, size }` points at a binary: `path` is tried on every
  mirror first, then the absolute `urls` (e.g. GitHub release assets for large files).
- AFK platform keys: `windows-x86_64`, `windows-aarch64`, `linux-x86_64`, `linux-aarch64`,
  `macos-aarch64`, `macos-x86_64` (`<os>-<arch>` as Rust spells them). A client uses the build
  for its own key and ignores the others, so new keys are additive.
- `afk`: `{ version, builds, minecraftVersions?, recommendedMinecraftVersion? }`. The AFK client's
  CI publishes the same object as `afk-release.json` (without the recommendation), which Studio
  imports.
  - `minecraftVersions`: the Minecraft versions this client build supports, newest first, for
    the AFK accounts' version picker. `value` is what the client's `MinecraftVersion` setting
    takes, `label` is shown. Values are 1–32 bytes without whitespace or control characters and
    never `auto` (clients offer that choice themselves), labels 1–64 characters without control
    characters, values are unique, at most 256 entries. Missing or empty: clients use their
    built-in list.
  - `recommendedMinecraftVersion`: the version the Projects server needs, set in Studio: one of
    the listed values or `auto` (the client detects the server's version).
  - A client that finds the list or the recommendation invalid ignores both and uses its
    built-in list; Studio refuses to publish them.
- `packs[]`: `{ id, name, latest, manifest, family?, minecraft?, default? }`, one entry per
  published pack; `latest` is the `versionId` of the referenced manifest. See
  [Pack families](#pack-families) for the last three.
- `modSets` (optional `DocRef`): the ready-made mod sets, see [Mod sets](#mod-sets). Absent
  when the team publishes none. Clients download it when they first need it (the "Hazır set"
  choices), not with every refresh, and keep it in the document cache.

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
2. `schema` must be supported; every `DocRef` must be well formed; the pack family rules hold.
3. Rollback protection: `sequence` must be greater than the cached index's sequence. An equal
   sequence is accepted only when the bytes are identical (same publish from another mirror).
4. `config` and `news` are downloaded (preferring the same mirror) and verified before the new
   index replaces the cached one.

Studio sets `sequence = max(previous + 1, unix seconds of generatedAt)`, so it keeps growing
even if Studio lost track of the previous publish.

### Pack families

A family is one pack published for several Minecraft versions. Each member is an ordinary pack
(its own `id`, manifest history and `latest`) and the members share a `family` id. A pack
without `family` is its own family (`family` defaults to `id`), so indexes without these fields
keep their meaning.

- `minecraft`: `dependencies.minecraft` of the `latest` manifest. Studio copies it so clients
  can list a family's versions without downloading manifests. Optional for old indexes.
- `default` (default `false`): the member new installs use. Without one, new installs use the
  newest member: highest `minecraft`, then highest `latest` (semver), then the first in
  `packs`; members without `minecraft` count as oldest.
- Minecraft version order: the leading digits-and-dots part is compared numerically, component
  by component (`1.21.10` > `1.21.9`, `26.1` > `1.21.10`, missing components are 0); with equal
  numbers the plain release is newer than suffixed versions (`1.21.4` > `1.21.4-rc1` >
  `1.21.4-pre1`, suffixes compare as text); ids without a leading number are the oldest.
- Rules (an index that breaks them is refused): family ids are 1–64 characters from
  `[A-Za-z0-9._-]`; at most one `default` per family; each `minecraft` value at most once per
  family.
- The launcher config's `recommendedPack` names a pack id or a family id. The desktop launcher
  uses that family's default member for new installs. The Android app and desktop builds from
  before families treat it as a pack id only and install exactly that pack (else the first
  pack's family default, or the first pack), so point it at a member's id: Studio always writes
  the default member's id.
- A new `latest` of the same pack that changes `dependencies.minecraft` is an ordinary update
  (files are replaced in place). Moving an instance between members is a version switch and
  keeps one set per version ([Per-version sets](#per-version-sets)).

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
      "url": "https://projects.gg/turnuva",
      "join": true
    },
    {
      "id": "discord-1300000000000000001",
      "title": "Kış etkinliği finali",
      "startsAt": "2026-10-18T17:00:00Z",
      "endsAt": "2026-10-18T21:00:00Z",
      "newsId": "discord-1300000000000000001",
      "priority": 1,
      "feedOverride": {
        "fingerprint": "6f1c2a9e0b7d4c3f8a5e1d2b9c0f7a6e3d4b5c6a7f8e9d0c1b2a3f4e5d6c7b8a",
        "sourceUrl": "https://discord.com/channels/480094926164590593/480099230275600394/1300000000000000001"
      }
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
    "clientId": "1560400835589181552",
    "templates": {
      "idle": { "details": "Launcher'da", "largeImage": "logo", "largeText": "Projects Launcher" },
      "downloading": { "details": "{instance} indiriliyor", "state": "%{percent}" },
      "inGame": {
        "details": "{instance} · {version}",
        "state": "{server}",
        "largeImage": "logo",
        "largeText": "Projects Launcher",
        "buttons": [{ "label": "Sunucuya katıl", "url": "https://projects.gg/oyna" }]
      },
      "afk": { "details": "{afkCount} AFK hesap" }
    }
  },
  "features": { "afk": true, "premiumLogin": true, "offlineAccounts": true, "modrinthBrowser": true },
  "recommendedPack": "projects",
  "recommendedMinecraftVersion": "26.1.2",
  "newsFeed": {
    "enabled": true,
    "mirrors": ["https://projects.gg/launcher-feed/v1/"],
    "keys": [{ "keyId": "0fe230f49f15fe22", "publicKey": "At6bZeNxJfdJrokZ7DfOpAZpijAgpvh/7VUKEHSc29s=" }],
    "maxAgeDays": 60
  }
}
```

Every field is optional (`{}` is a valid config). Defaults: no maintenance, no banners, bundled
hero art with `particles: embers`, `season: none`, all feature flags `true` except `friends`
and `teams` (`false`), Discord enabled with the launcher's built-in client id when `clientId` is empty, no
news feed, the launcher's built-in recommended Minecraft version.

- `banners[].level`: `info | warning | event | critical`; a banner is shown while
  `startsAt <= now < endsAt` (missing bounds are open). `dismissible` defaults to `true`.
- `hero.particles`: `embers | sparks | snow | none`. `theme.season`:
  `none | halloween | winter | holiday | event`. Unknown values fall back to the defaults.
- `cta`: `{ label, url? , newsId? }` — `newsId` opens an article, `url` opens the browser.
- `events[]`: countdown until `startsAt`, "live" until `endsAt`, hidden afterwards. `newsId`
  (optional): the article the event card opens while that item is on the home screen (else
  the card falls back to `url`). `priority` (optional integer, default 0): which of several
  live events the card shows ([event card](#event-card)). `join: true` (optional boolean,
  default `false`): the event takes place on the server; while it is live the card offers
  "Katıl" ([event card](#event-card)). Launchers that do not know the field ignore it, so it
  needs no new `schema` or `minLauncherVersion`. `hidden: true`: never shown; Studio
  publishes a hidden event to take the feed event with its id off the card. `feedOverride`:
  Studio's record of the feed event this entry replaces or hides, ignored by launchers (see
  [Studio overrides](#studio-overrides)). These are left out when unused (`priority` while 0,
  `join` and `hidden` while `false`).
- `maintenance` is active while `enabled` and (`until` missing or `now < until`).
- `minLauncherVersion`: older launchers show the lock screen and install the update.
- `recommendedMinecraftVersion`: the Minecraft version launchers recommend. Their version lists
  (creating an instance, switching an instance's version) mark it "Önerilen" and list it even
  while snapshots are hidden; new custom instances start with it selected. The official pack's
  list marks it when the pack is published for it, otherwise the family's default member. A
  Mojang version id: 1–32 bytes without whitespace or control characters (snapshots allowed).
  Missing, empty or invalid: launchers use their built-in default (`26.1.2` in desktop 2.0,
  `pl_content::DEFAULT_RECOMMENDED_MINECRAFT_VERSION`). A version Mojang does not list gets no
  mark. A new value reaches running launchers with the next content refresh. Writers omit the
  field when empty, so existing configs keep their exact shape. Studio refuses an invalid value
  and warns when Mojang does not list it or when the recommended pack's family has a member for
  it that is not the family's default. The AFK accounts' version (`afk.recommendedMinecraftVersion`
  in the index) is a separate setting.
- `features.friends`: the optional friends feature (Discord sign-in, friend list, presence,
  "Birlikte oyna"). Off by default; while it is `false` (or missing) launchers show nothing of
  it and never contact the friends service. Turn it on only once `social.projects.gg` is live.
  Writers omit it while it is `false`, so existing configs keep their exact shape.
- `features.teams`: teams inside the friends feature (create or join a team, invites, the team
  section in the friends panel, teammates' presence). Off by default and only shown while
  `features.friends` is on too. Launchers also hide it while the friends service does not
  announce team limits (a service without the teams routes). Friend categories need no flag:
  they follow `features.friends` and the service's support. Turn it on only after the service
  with the teams routes and its database migration is live. Writers omit it while it is
  `false`. Launchers that do not know the field ignore it. Studio sets it with "Takımlar" next
  to "Arkadaşlar" (it can't be switched on while `friends` is off) and warns before a publish
  that switches it on, or that has it on without `friends`.
- `newsFeed`: the Discord bot's feed ([News feed (Discord bot)](#news-feed-discord-bot)).
  `mirrors`: 1–4 distinct `https://` bases with a host, ending in `/`, without credentials, query
  or fragment. `keys`: 1–4 distinct `{ keyId, publicKey }`, `publicKey` the base64 of a 32-byte
  Ed25519 public key and `keyId` its id as for `index.json.sig`. `maxAgeDays`: 1–365, default 60.
  Missing or `enabled: false`: launchers never fetch the feed and show nothing of it. Writers
  omit the object while it is disabled, so existing configs keep their exact shape. Studio
  refuses to publish an enabled feed that breaks these rules; launchers use the valid mirrors
  and keys and the default for an out-of-range `maxAgeDays`.
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
- Visible when not `hidden`, `publishedAt <= now` and (`expiresAt` missing or
  `now < expiresAt`), so posts can be scheduled.
- Order: pinned items first, by `pinOrder` (1 first; pinned items without one come after those
  with one), then newest `publishedAt`, then `id`. `pinOrder` means nothing on an item that is
  not pinned. Studio writes `1…n` for the pinned items in the order the admin dragged them; a
  newly pinned item starts at the top.
- `featured` items (and the hero) are prefetched for offline display.
- `hidden: true`: never shown. Studio publishes hidden items only to take an item of the
  Discord bot's feed off the home screen ([Studio overrides](#studio-overrides)); it also sets
  `expiresAt` to `publishedAt` on them, so readers older than this field never show them
  either. `pinOrder`, `hidden` and `feedOverride` are left out when unused.

<!-- example: NewsFeed -->
```json
{
  "schema": 1,
  "items": [
    {
      "id": "kis-sezonu",
      "title": "Kış sezonu başladı",
      "summary": "Yeni harita ve günlük görevler.",
      "body": "Kış haritası açıldı.",
      "category": "announcement",
      "tags": [],
      "publishedAt": "2026-10-14T09:00:00Z",
      "pinned": true,
      "pinOrder": 1,
      "featured": false
    },
    {
      "id": "discord-1300000000000000002",
      "title": "2.4 yama notları",
      "summary": "Yeni zindan, PvP dengelemeleri ve düzeltmeler.",
      "body": "- Yeni zindan: Kül Mağarası\n- Kılıç hasarı dengelendi",
      "category": "patchNotes",
      "tags": [],
      "publishedAt": "2026-10-15T18:30:00Z",
      "pinned": true,
      "pinOrder": 2,
      "featured": false,
      "author": "Projects Ekibi",
      "feedOverride": {
        "fingerprint": "0d9f3b7a5c1e2f4a6b8c0d2e4f6a8b0c2d4e6f8a0b2c4d6e8f0a2b4c6d8e0f2a",
        "sourceUrl": "https://discord.com/channels/480094926164590593/480099524652695576/1300000000000000002"
      }
    },
    {
      "id": "discord-1300000000000000003",
      "title": "test 123",
      "summary": "",
      "body": "",
      "category": "announcement",
      "tags": [],
      "publishedAt": "2026-10-16T07:00:00Z",
      "expiresAt": "2026-10-16T07:00:00Z",
      "pinned": false,
      "featured": false,
      "hidden": true,
      "feedOverride": {
        "fingerprint": "9a8b7c6d5e4f3a2b1c0d9e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8b"
      }
    }
  ]
}
```

Above: the admin pinned the season post first and Studio's copy of the bot's 2.4 patch notes
second, and hid the bot's `test 123`. Launchers show `kis-sezonu`, then the copy, then the
feed's other items by date; `discord-1300000000000000003` is shown by no one.

## News feed (Discord bot)

The Discord bot summarizes the admins' posts in the server's announcement and changes channels
and publishes them as a signed feed on its own mirrors. Studio delegates the right to sign it
through the launcher config (`newsFeed.keys`), so the content signing key never leaves Studio
and a compromised bot can only touch news; keys are added, rotated and revoked from Studio
without a launcher release.

```text
<feed mirror>/feed.json              BotFeed: the only mutable file, a full snapshot
<feed mirror>/feed.json.sig          { keyId, signature } with a key from newsFeed.keys
<feed mirror>/media/<sha256>.webp    immutable images referenced by items
```

Feed mirror bases come from `newsFeed.mirrors` (e.g. `https://projects.gg/launcher-feed/v1/`).
The signature envelope and rules are those of `index.json.sig`: Ed25519 over the exact bytes of
`feed.json`, standard base64 with padding, `keyId` the key's id.

<!-- example: BotFeed -->
```json
{
  "schema": 1,
  "sequence": 1792130400,
  "generatedAt": "2026-10-16T09:00:00Z",
  "items": [
    {
      "id": "discord-1300000000000000001",
      "title": "Kış etkinliği başlıyor",
      "summary": "Cumartesi 20.00'de yeni harita ve ödüllerle başlıyor.",
      "body": "## Neler var\n\n- Yeni harita\n- Günlük ödüller",
      "cover": {
        "path": "media/3fa405a8301ace34d11cf44a816080b8f0e49a48fbd048b8aef1543a8c58bdb6.webp",
        "sha256": "3fa405a8301ace34d11cf44a816080b8f0e49a48fbd048b8aef1543a8c58bdb6",
        "size": 98304,
        "kind": "image",
        "width": 1280,
        "height": 720,
        "blurhash": "L6PZfSi_.AyE_3t7t7R**0o#DgR4"
      },
      "category": "event",
      "tags": [],
      "publishedAt": "2026-10-16T08:58:00Z",
      "author": "Projects Ekibi",
      "source": {
        "kind": "discord",
        "channel": "duyurular",
        "url": "https://discord.com/channels/480094926164590593/480099230275600394/1300000000000000001"
      }
    },
    {
      "id": "discord-1300000000000000002",
      "title": "2.4 yama notları",
      "summary": "Yeni zindan ve PvP dengelemeleri.",
      "body": "- Yeni zindan: Kül Mağarası\n- Kılıç hasarı dengelendi",
      "category": "patchNotes",
      "tags": [],
      "publishedAt": "2026-10-15T18:30:00Z",
      "author": "Projects Ekibi",
      "source": {
        "kind": "discord",
        "channel": "değişiklikler",
        "url": "https://discord.com/channels/480094926164590593/480099524652695576/1300000000000000002"
      }
    }
  ],
  "events": [
    {
      "id": "discord-1300000000000000001",
      "title": "Kış etkinliği",
      "startsAt": "2026-10-18T17:00:00Z",
      "endsAt": "2026-10-18T20:00:00Z",
      "newsId": "discord-1300000000000000001",
      "source": {
        "kind": "discord",
        "channel": "duyurular",
        "url": "https://discord.com/channels/480094926164590593/480099230275600394/1300000000000000001"
      }
    },
    {
      "id": "discord-1300000000000000004",
      "title": "Yeni oyuncu haftası",
      "startsAt": "2026-10-16T00:00:00Z",
      "endsAt": "2026-10-23T00:00:00Z",
      "priority": -1,
      "source": {
        "kind": "discord",
        "channel": "duyurular",
        "url": "https://discord.com/channels/480094926164590593/480099230275600394/1300000000000000004"
      }
    }
  ],
  "status": {
    "level": "outage",
    "title": "Bazı hizmetlerde sorun var",
    "message": "Şu an erişilemiyor: Survival Zephyr",
    "services": [{ "name": "Survival Zephyr", "state": "down" }],
    "url": "https://status.projects.gg",
    "updatedAt": "2026-10-16T08:59:00Z",
    "expiresAt": "2026-10-16T09:14:00Z"
  }
}
```

- `items[]`: the shape and rules of `NewsFeed.items[]` plus `source`; `cover.path` is relative
  to the feed mirror. Ids are `discord-<messageId>` and edits keep them. `pinned` is never set
  by the bot; clients ignore it.
- `events[]`: the shape of `LauncherConfig.events[]` with `endsAt` optional, plus `newsId` (the
  item the event came from) and `source`. An event without `endsAt` is live for 3 hours after
  `startsAt` and is shown without time left; a bot that knows the end writes `endsAt`.
  `priority` (optional integer, default 0) works as for config events: above, the week-long
  "Yeni oyuncu haftası" has `-1`, so any other live event leads the card over it, even one
  that started before it. `join` (optional boolean, default `false`) works as for config
  events. The bot does not write it yet; an admin marks a Discord event as on the server through
  a Studio copy ("Sunucuda", [Studio overrides](#studio-overrides)), and a bot that learns it
  later only has to add the field.
- `source`: `{ kind, channel?, url? }`: `kind` is `discord` (unknown kinds are tolerated),
  `channel` a display name, `url` the original message (`https://` only).
- `sequence` strictly increases (`max(previous + 1, unix seconds)`). The feed is a full
  snapshot: a newer feed without an item means it is gone (deleted on Discord or hidden by an
  admin).
- `status` (optional): a short-lived notice about an active problem on the server's status
  page ([Status notice](#status-notice)); missing while everything works.
- `minecraft` (optional): Minecraft's own updates, summarized by the bot from Mojang's public
  launcher content ([Minecraft news](#minecraft-news)). A separate array, so launchers that do not
  know it never show these entries as Projects news; they only fill places Projects news leaves
  empty. Missing when there are none.

Client rules:

1. No request and nothing shown while `newsFeed` is missing or disabled (a cached feed is
   deleted).
2. Mirrors are tried in order. `feed.json` is requested with the mirror's last `ETag`
   (`304` = unchanged, no signature download). On a signature mismatch both files are fetched
   once more with `?t=<ms>`, then the next mirror is tried. The key must be in `newsFeed.keys`.
3. Whole-feed rules; breaking one keeps the cached feed: `feed.json` at most 512 KB and
   `.sig` at most 16 KB, a supported `schema`, at most 50 items and 20 events, and a `sequence`
   greater than the cached feed's (equal only with identical bytes).
4. Entry rules; breaking one affects only that entry. Dropped: an entry that does not parse
   (also an event whose `priority` is not an integer or whose `join` is not a boolean),
   an id that is not 1–64 characters from `[A-Za-z0-9._-]`, a repeated id (the first wins), an
   empty title, more than 120 characters of title, 300 of summary or 8 000 of body (characters
   are Unicode scalar values), an event with `endsAt <= startsAt`. Removed field: a cover that
   is not `kind: image` of 1 byte to 2 MB with a valid path and SHA-256 (posters are ignored), a
   `cta` without a label and exactly one target (a safe link or a news id), an event `url` that
   is not a safe link, a `source.url` that is not `https://`, a `newsId` naming no item of the
   feed. `status` has its own rules ([Status notice](#status-notice)); breaking them drops only
   the notice. `minecraft` has its own rules ([Minecraft news](#minecraft-news)); breaking them
   drops only that entry (or, for the array itself, only the array).
5. Safe links are `https://` URLs with a host and no credentials, and `projects://` deep links.
   Other links in `body` are shown as plain text; images in a feed `body` are not loaded.
6. Covers are fetched only from the feed mirrors (the one that served the feed first), read up
   to their declared size and kept only when size and SHA-256 match.
7. The cached feed is verified again against the current `newsFeed.keys` whenever it is read:
   removing a key also drops what it signed, including its `sequence` floor.
8. A feed failure never breaks the home screen: the cached feed, or Studio's content alone, is
   shown.

Home screen (Studio always wins):

- News column: Studio's visible news plus the feed's visible items published at most
  `maxAgeDays` ago whose id Studio does not publish. A Studio item hides the feed item with its
  id even while it is not visible itself (`hidden`, scheduled or expired). Order as for
  `NewsFeed` (pinned Studio items in their `pinOrder`, then newest; feed items are never
  pinned).
- Patch notes box: the newest visible item (by `publishedAt`, then id) with
  `category: patchNotes` from either source; pinning does not change it. Without one, the
  official pack's changelog.
- Event card: config and feed events merged. Hidden config events are left out, and a feed
  event whose id a config event uses (hidden or not) is ignored. Which event the card shows:
  [Event card](#event-card). An event's `newsId` (config or feed) opens that article while it
  is on the home screen.
- Status notice: shown where the maintenance notice goes, only while no maintenance notice is
  active ([Status notice](#status-notice)).
- Minecraft news: only in the places the news above leaves empty, never in the hero, the patch
  notes box or the event card ([Minecraft news](#minecraft-news)).

### Event card

An event is live while `startsAt <= now < end` (`end` is `endsAt`, or `startsAt` + 3 hours for
a feed event without one), upcoming before that and gone after it. The card orders the events
that are not gone and shows the first:

1. Live events before upcoming ones.
2. Live events: the higher `priority` first, then the later `startsAt`. A short event that
   starts during a long one overlays it while it runs; when it ends, the long one is back.
   Raising the long one's priority keeps it in front; lowering an event's priority makes it
   yield to every other live event.
3. Upcoming events: the sooner `startsAt` first, then the higher `priority`.
4. Ties: config before feed, then the smaller id.

A live event shows a countdown to its end (an open-ended feed event shows when it started
instead), an upcoming one a countdown to its start. Launchers apply the rule again whenever an
event starts or ends, so the card changes on time without a refresh, and list the other live
and upcoming events behind the card.

"Katıl" (`join: true`, config or feed event): while the event on the card is live, the card
offers "Katıl". It starts the official Projects instance (created first if the player has none)
and joins the server the instance joins by itself (`play.projects.gg`), whatever the player's
auto-join setting, exactly like "Katıl" in the friends list: the usual launch with its account
and AFK questions, and nothing while another game is being prepared or played (one game at a
time). Upcoming events never offer it (the reminder bell is there instead), and the event
never starts the game by itself. Without `join` the card looks as before.

### Status notice

`status` tells players about a problem on the server's status page while it lasts: services
that are down or reachable only in part, or a warning posted on the status page. The bot writes
it only while there is such a problem (example in [News feed (Discord bot)](#news-feed-discord-bot)).

| Field | Rule |
| --- | --- |
| `level` | `outage` (services down), `degraded` (services reachable only in part) or `notice` (a warning posted on the status page). Missing or unknown: `notice`. |
| `title` | Required, 1–80 characters, not blank. |
| `message` | Optional, at most 200 characters; default empty. |
| `services` | Optional, at most 20 `{ name, state }`: `name` 1–64 characters, not blank; `state` `down` or `degraded` (unknown: `unknown`). |
| `url` | Optional: the status page, an `https://` URL with a host and no credentials. |
| `updatedAt` | Required: when the bot last confirmed the notice. |
| `expiresAt` | Required: after `updatedAt`, at most 15 minutes after it. |

- Characters are Unicode scalar values. Texts are plain (no Markdown), short and in the players'
  language; the bot writes them.
- Unknown fields are ignored here as everywhere. A `status` that breaks a rule of the table
  (wrong type, missing or blank title, a limit exceeded, a bad expiry) is ignored on its own: the
  rest of the feed is used as usual. A `url` that is not `https://` is removed and the notice
  kept.
- Clients show the notice only while `now < expiresAt`; an expired one is ignored, also in a
  cached feed, so a bot that stops leaves no stale notice behind. While the problem lasts, the
  bot publishes the notice again before it expires (new `updatedAt` and `expiresAt`, a new
  `sequence`); when the problem is over, it publishes the feed without `status`. It publishes
  only on a change or to renew the notice, debounced.
- Studio's maintenance notice (`LauncherConfig.maintenance`) wins: while it is active, the status
  notice is not shown. The notice is informational and never blocks playing. Launchers show
  `title`, `message` and a link to `url`; `services` is for readers that list the services.

### Minecraft news

`minecraft` carries Minecraft's own updates so the home screen is not empty when the server has
little news. The bot reads Mojang's public launcher content (Java Edition patch notes), writes a
short summary in the players' language in its own words, and links the official article. The
entries live in their own array: launchers that do not know it ignore it, so they never appear as
Projects news, and launchers that know it only put them where Projects news leaves room.

<!-- example: BotFeed -->
```json
{
  "schema": 1,
  "sequence": 1792310400,
  "generatedAt": "2026-10-18T10:00:00Z",
  "items": [],
  "events": [],
  "minecraft": [
    {
      "id": "minecraft-26.3",
      "version": "26.3",
      "kind": "release",
      "title": "Minecraft 26.3 çıktı: Wilderness Bound",
      "summary": "Dappled Forest biyomu, kavak ağaçları ve terk edilmiş kamplar Java sürümüne geldi.",
      "body": "Keşif temalı yeni oyun güncellemesi Java sürümünde.\n\n## Öne çıkanlar\n\n- **Dappled Forest** biyomu ve üç renk yapraklı **Poplar** ağaçları\n- **Abandoned Camp** yapıları ve yeni kâşif haritaları\n- Kamp için **Cushion** ve **Straw Bed**",
      "cover": {
        "path": "media/6d1c0f8b2a4e93c7d5b1f0a8e2c4d6b8f0a2c4e6d8b0f2a4c6e8d0b2f4a6c8e0.webp",
        "sha256": "6d1c0f8b2a4e93c7d5b1f0a8e2c4d6b8f0a2c4e6d8b0f2a4c6e8d0b2f4a6c8e0",
        "size": 84211,
        "kind": "image",
        "width": 1280,
        "height": 720,
        "blurhash": "LKO2?U%2Tw=w]~RBVZRi};RPxuwH"
      },
      "publishedAt": "2026-09-15T11:23:02Z",
      "expiresAt": "2026-10-30T11:23:02Z",
      "source": {
        "kind": "minecraft",
        "url": "https://www.minecraft.net/article/minecraft-java-edition-26-3"
      },
      "translations": {
        "en": {
          "title": "Minecraft 26.3 is out: Wilderness Bound",
          "summary": "The Dappled Forest biome, Poplar trees and abandoned camps arrive in Java Edition.",
          "body": "A new exploration game drop is out in Java Edition.\n\n## Highlights\n\n- The **Dappled Forest** biome with **Poplar** trees in three leaf colours\n- **Abandoned Camp** structures and new explorer maps\n- **Cushions** and **Straw Beds** for your camp"
        }
      }
    }
  ]
}
```

| Field | Rule |
| --- | --- |
| `id` | Required: `minecraft-` followed by `version`; at most 64 characters from `[A-Za-z0-9._-]`; unique in the array. |
| `version` | Required: Mojang's version id (`26.3`, `26.1.2`, `26.4-snapshot-4`), 1–40 characters from `[A-Za-z0-9._-]`. |
| `kind` | `release`, or `snapshot` for snapshots, pre-releases and release candidates. Missing or unknown: `snapshot`. |
| `title`, `summary`, `body` | As in `items[]`: title required, not blank, at most 120 characters; summary at most 300; body Markdown, at most 8 000. Written in the feed's language (Turkish). |
| `cover` | Optional: a `MediaRef` relative to the feed mirror, with the rules of item covers. |
| `publishedAt` | Required: when Mojang released the version. |
| `expiresAt` | Required: after `publishedAt` and at most 90 days after it. |
| `source` | Required: `{ "kind": "minecraft", "url" }`; `url` is the official article, an `https://` URL whose host is `minecraft.net` or `www.minecraft.net`. |
| `translations` | Optional: texts in other languages by language code; today only `en`: `{ "title", "summary"?, "body"? }`, each with the limits of the field it replaces. |

Rules (characters are Unicode scalar values; unknown fields are ignored as everywhere):

1. The array: missing means none. Not an array, or more than 10 entries: ignored as a whole; the
   rest of the feed is used. Entries do not count toward the 50 items.
2. An entry is dropped when it does not parse, its `id` breaks the rule above or repeats one
   before it, `version` is missing or invalid, the title breaks its rules, the summary or body is
   too long, `publishedAt` or `expiresAt` is missing, `expiresAt` is not after `publishedAt` or
   more than 90 days after it, or `source.url` is missing or not an `https://` URL on
   `minecraft.net` (the attribution link is part of the entry). A cover that breaks the item
   cover rules is removed. A translation that breaks its limits, or is not an object, is ignored
   for its language only. `category`, `tags`, `author`, `cta`, `pinned` and `featured` are never
   written by the bot; clients ignore them.
3. Links in `body` follow the item rules (only `https://` and `projects://` are clickable); the
   bot writes none: the article is linked through `source.url`.
4. Visible while `publishedAt <= now < expiresAt` and `publishedAt` is at most `maxAgeDays` ago.
   A Studio news item with the same id hides the entry, even while that item is not visible
   itself (Studio wins, as for `items[]`; [Studio overrides](#studio-overrides)).
5. Order among themselves: the newer `publishedAt` first, then the smaller id.
6. Language: the base texts are Turkish. A launcher whose interface language is English shows
   `translations.en` when it has one; a missing `summary` or `body` there falls back to the base
   text.
7. Covers of `minecraft[]` entries are feed covers: fetched from the feed mirrors, verified by size
   and SHA-256 and cached like item covers. Without a cover the launcher shows its own art.

Where launchers show them:

- Never as the hero, in the patch notes box (they are not the server's patch notes), on the event
  card, pinned or featured. They never raise a notification, a reminder, an unread badge or a
  "new" dot, and they do not move the "seen" mark of Projects news.
- **Desktop home news column:** the rows Projects news fills come first (Studio's and the feed's
  news minus the hero's article and the patch notes box's, at most 4 rows). Only rows left empty
  get Minecraft entries, in their order. With no Projects news and no patch notes, Minecraft rows
  take the place of the "no news" line.
- **News page ("Tüm haberler"):** every Projects news item first (the large first card is always
  Projects news while there is any), then the Minecraft entries under a "Minecraft" heading. A
  "Minecraft" filter shows only them; the category filters never show them.
- **Mobile Projects card:** the story is the latest Projects story; only when there is none, the
  first Minecraft entry. The news list orders them as the desktop news page does.
- Label: "Minecraft" in place of the category. The reader names the source ("minecraft.net") and
  links `source.url` ("minecraft.net'te oku"), opened in the system browser (`https://` only),
  never with "Discord'da aç".

What the bot publishes (informative): Java Edition releases for 45 days after their release, at
most 3 at a time; of several hotfixes of one game drop only the newest (26.1 and 26.1.2, not
26.1.1); snapshots only when the owner turns them on. Admins hide an entry from Discord (the
notice in the review channel, `/launcher kaldır`) or through the bot's API; a hidden entry is
simply missing from the next feed. Studio hides one with its own signed content instead
([Studio overrides](#studio-overrides)): the entry stays in the feed and launchers leave it out.
ProBot: `discord-bot/docs/launcher-feed.md` → "Minecraft news".

### Studio overrides

Studio controls the bot's items and events without the bot's key, through its own signed
content and the "Studio wins" rule:

- **Edit:** Studio publishes its own `NewsItem` (or `EventCountdown`) with the feed entry's id,
  filled from the feed entry; launchers show Studio's copy in its place. A copy may be pinned
  and ordered like any Studio item; an event copy carries its own `priority` and `join`. Its
  cover, if any, is re-hosted on the content mirrors.
- **Hide:** Studio publishes an entry with the feed entry's id and `hidden: true` (news items
  also get `expiresAt` = `publishedAt`, see [News](#news)). Hidden items carry no cover.
- **Undo:** Studio stops publishing the entry; the feed entry shows again.
- The same works for a `minecraft[]` entry by its id ([Minecraft news](#minecraft-news)). Studio
  only hides these: its hidden item carries the entry's id, title and `publishedAt` (as
  `expiresAt` too), `category: update` and no text or cover; launchers that know `minecraft`
  leave the entry out, readers that do not never show either. A Studio copy of one (an item with
  the id that is not hidden) would be an ordinary Studio news item: it shows as Projects news.
- `feedOverride: { fingerprint, sourceUrl? }` on the Studio entry records which feed entry it
  replaced or hid: `fingerprint` is Studio's digest of the feed entry's content when the admin
  took it over (an opaque string; Studio compares it with the current feed entry to report that
  the original changed since; for a `minecraft[]` entry it covers the texts, the English texts,
  the cover and `publishedAt`, so a new `expiresAt` is no change), `sourceUrl` the original
  message, or a `minecraft[]` entry's article. Launchers ignore the object; it is published so
  every admin (after "pull from live" or a rollback) sees the same.

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
      "path": "mods/fabric-api-0.116.6+1.21.1.jar",
      "hashes": {
        "sha1": "c4a6b98922058d3b769e3f702b75863b87652188",
        "sha512": "ab6230d9ffd0f1270f30734e9722b191fd5313c446e47ddb1f0ea7db27141672f3052967a98700b1a97b3e41c8e5cd4664733dbdcb079c0eb400399cf3978d43"
      },
      "env": { "client": "required", "server": "required" },
      "downloads": ["https://cdn.modrinth.com/data/P7dR8mSH/versions/9YVrKY0Z/fabric-api-0.116.6%2B1.21.1.jar"],
      "fileSize": 2306867
    },
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
      "path": "mods/iris-fabric-1.8.8+mc1.21.1.jar",
      "hashes": {
        "sha1": "b1139bd80274fe91463135b45776c0ac3da2eb5a",
        "sha512": "9ff15fbfb264d3043feb3f634f2cab8e19789a14b3934058d50306cffca8ffc87d26c82748e734640441e658786f0e67316addc8f6b74d5adf9bac65f1128601"
      },
      "env": { "client": "required", "server": "unsupported" },
      "downloads": ["https://cdn.modrinth.com/data/YL57xq9U/versions/kuOV4Ece/iris-fabric-1.8.8%2Bmc1.21.1.jar"],
      "fileSize": 3145728
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
    "family": "projects",
    "changelog": "## 2.3.0\n\n- Sodium 0.6\n- Yeni shader paketi (isteğe bağlı)",
    "items": [
      {
        "id": "P7dR8mSH",
        "path": "mods/fabric-api-0.116.6+1.21.1.jar",
        "title": "Fabric API",
        "category": "library",
        "modrinth": { "projectId": "P7dR8mSH", "versionId": "9YVrKY0Z" },
        "library": true
      },
      {
        "id": "AANobbMI",
        "path": "mods/sodium-fabric-0.6.0+mc1.21.1.jar",
        "title": "Sodium",
        "summary": "Daha yüksek FPS, daha akıcı oyun.",
        "category": "performance",
        "iconUrl": "https://cdn.modrinth.com/data/AANobbMI/icon.png",
        "pageUrl": "https://modrinth.com/project/AANobbMI",
        "modrinth": { "projectId": "AANobbMI", "versionId": "u1OEbNKx" },
        "defaultEnabled": true,
        "requires": ["P7dR8mSH"]
      },
      {
        "id": "YL57xq9U",
        "path": "mods/iris-fabric-1.8.8+mc1.21.1.jar",
        "title": "Iris",
        "summary": "Shader paketlerini çalıştırır.",
        "category": "visual",
        "modrinth": { "projectId": "YL57xq9U", "versionId": "kuOV4Ece" },
        "defaultEnabled": false,
        "requires": ["AANobbMI"]
      },
      {
        "id": "HVnmMxH1",
        "path": "shaderpacks/ComplementaryReimagined_r5.4.zip",
        "title": "Complementary Reimagined",
        "summary": "Gerçekçi ışık ve gölgeler; güçlü bir ekran kartı ister.",
        "category": "visual",
        "modrinth": { "projectId": "HVnmMxH1", "versionId": "xT4e9w1L" },
        "defaultEnabled": false,
        "requires": ["YL57xq9U"]
      }
    ],
    "optionalGroups": [
      {
        "id": "shaders",
        "name": "Shader paketi",
        "description": "Güçlü ekran kartları için gölgelendiriciler.",
        "defaultEnabled": false,
        "files": ["mods/iris-fabric-1.8.8+mc1.21.1.jar", "shaderpacks/ComplementaryReimagined_r5.4.zip"]
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
  on clients). Optional files that are in neither an item nor a group are installed.
- `projects.family`: the pack family (see [Pack families](#pack-families)); defaults to
  `packId`. Same id rules as in the index.
- `projects.items`: the mod center's catalog. **Every item is optional**: the client installs
  an item's file only when the player selected the item or an enabled item requires it (see
  [Sync semantics](#sync-semantics-and-pack-state)). Files no item covers (configs,
  `options.txt`, …) behave as before. A manifest without `items` behaves exactly as before.
  - `id`: stable id, 1–64 characters from `[A-Za-z0-9._-]`. Studio uses the Modrinth project
    id (or a slug for our own files). Choices are keyed by it, so a new file name for the same
    mod keeps the player's choice.
  - `path`: the `files[]` entry the item installs (one item per file).
  - `title`, `summary` (optional): shown in the mod center.
  - `category`: `performance | visual | interface | gameplay | library | other` (default and
    unknown values: `other`); display only.
  - `iconUrl` (optional): `https://` image. `modrinth` (optional): `{ projectId, versionId }`.
  - `pageUrl` (optional): the mod's official page, `https://`, which the mod center opens in the
    browser. Studio writes the Modrinth project page (`https://modrinth.com/project/<id>`) for
    items with a Modrinth reference unless the editor set another page; files we host get theirs
    by hand (authors' terms may require a link to their page). Clients without the field ignore
    it.
  - `defaultEnabled` (default `true`): selected on a brand-new install only.
  - `requires` (default `[]`): ids of items installed together with this one.
  - `library` (default `false`): dependency only; installed exactly when an enabled item
    requires it and never offered as a choice. Studio gives libraries `category: library`.
  - Rules (a manifest that breaks them is refused): unique ids; `path` exists in `files` and
    is not `unsupported` on the client; no two items share a path; `title` is not blank;
    `iconUrl` and `pageUrl` are HTTPS; `requires` names existing items and has no cycles.
- `optionalGroups` (legacy): switchable sets of `files[]` paths. A file in several groups is
  installed when any of them is enabled. The player's choice is remembered in the pack state;
  new groups start at `defaultEnabled`. Files covered by an item ignore groups; groups still
  switch the other files. A client without item support (no released desktop or Android build
  lacks it) would ignore `items` and install every file outside a disabled group, so Studio also
  lists default-off items in a default-off group, as `shaders` does above: on every build it adds
  the group `mod-center-optional` ("İsteğe bağlı modlar", `defaultEnabled: false`) with every
  item a fresh install leaves out (default-off items and libraries only they require) that none
  of the pack's own groups lists. Studio drops that group when it reads a manifest back, so the
  id is reserved.
- `configPolicy`: `rules` are checked top to bottom, first match wins, otherwise `default`.
  Policies: `overwrite` (always match the pack), `ifMissing` (install only when absent),
  `preserveUserChanges` (update only while unchanged since we installed it). Unknown policies
  behave like `preserveUserChanges`.
- Glob syntax (case-sensitive, whole path): `*` = any characters except `/`, `**` = any
  characters including `/` (`**/` also matches zero folders), `?` = one character except `/`.
  `*.txt` matches only top-level files; use `**/*.txt` for any depth.

### Key bindings at the first launch

Not part of the format yet. A pack that ships its own `options.txt` decides the keys. Otherwise
both launchers write the official pack's key bindings (`crates/pl-mc/data/key-bindings.json`, the
mods' keys moved so that nothing shares a key in normal play) into the `options.txt` they create
at the first launch, never into an existing one by themselves: an older file that still has the
clashes (`clashesWith`) gets only those lines moved, and only when the player accepts. Studio may
take the table over later as a field of the manifest's `projects` extension, checked against the
pack's jars when it publishes.

### Path rules

`files[].path` is relative to the game directory and is rejected (the whole manifest fails)
when it is empty or longer than 512 bytes, starts with `/`, contains `\`, `:`, control
characters or `<>"|?*`, has empty, `.` or `..` components, has a component ending in `.` or
space, uses a Windows device name (`CON`, `PRN`, `AUX`, `NUL`, `COM0-9`, `LPT0-9`, also with an
extension), or starts with `.projects/`. Duplicate paths (case-insensitive) and a path that is
both a file and a folder are rejected too.

### Sync semantics and pack state

The launcher records the active set of a game directory in
`<game dir>/.projects/pack-state.json`:

<!-- example: PackState -->
```json
{
  "packId": "projects",
  "versionId": "2.3.0",
  "family": "projects",
  "minecraft": "1.21.1",
  "files": {
    "mods/fabric-api-0.116.6+1.21.1.jar": { "sha1": "c4a6b98922058d3b769e3f702b75863b87652188", "policy": "overwrite", "item": "P7dR8mSH" },
    "mods/sodium-fabric-0.6.0+mc1.21.1.jar": { "sha1": "6f07dc1d0a287f925d7672e6884d85ceb3f339ed", "policy": "overwrite", "item": "AANobbMI" },
    "mods/iris-fabric-1.8.8+mc1.21.1.jar": { "sha1": "b1139bd80274fe91463135b45776c0ac3da2eb5a", "policy": "overwrite", "item": "YL57xq9U", "disabled": true },
    "options.txt": { "sha1": "513f8de9259fe7658fe14d1352c54ccf070e911f", "policy": "ifMissing" }
  },
  "groups": { "shaders": false },
  "items": { "AANobbMI": true, "HVnmMxH1": false, "YL57xq9U": false },
  "seenItems": ["AANobbMI", "HVnmMxH1", "P7dR8mSH", "YL57xq9U"],
  "manual": { "YL57xq9U": "disabled" }
}
```

- `files`: path → the file as we installed it (`sha1`, `policy`, and `item` when an item
  installed it). Only files listed here are ever deleted or updated as "ours". `disabled`
  (default `false`): the player switched the item off by renaming the file, which sits at
  `<path>.disabled` and is kept while the item is off. `modified` (default `false`): at the
  last sync the file was not the one we installed (the player replaced it).
- `family`, `minecraft`: the set this state describes (empty in states written before
  families).
- `groups`: legacy group choices. `items`: the player's choices by item id. `seenItems`: the
  item ids the player has been shown. `manual`: item id → the manual change a sync adopted
  (`removed`, `disabled`, `enabled`) that the player has not confirmed in the mod center yet.
  All of them are optional.

Which items are installed (the mod center rules):

1. A fresh set has neither `items` nor `seenItems` (a new install, or a set created before the
   pack had items). It selects every item whose file the pack already installed (recorded in
   `files`, by `item` or by path) and otherwise follows `defaultEnabled`. Those choices are
   recorded and every item counts as seen.
2. After that, recorded choices win, also when Studio later changes `defaultEnabled`. An item
   without a recorded choice was added to the pack later: it stays off for this player and is
   *new* (not in `seenItems`) until the player confirms the mod center. Nothing new is
   installed automatically.
3. Changes the player made in the game directory are adopted, never fought (see below).
   Adopted choices rank below an explicit selection and above the recorded choices.
4. Confirming the mod center (applying a selection, even an unchanged one) records the
   choices the player made, adds every item of the manifest to `seenItems` and clears
   `manual`. Background syncs (before a launch) never change choices or `seenItems` except on
   a fresh set and by adopting manual changes.
5. Installed are the selected items plus, transitively, everything they `require`. A required
   item is installed even when the player switched it off (the mod center shows which enabled
   items need it). Libraries are installed exactly when an enabled item requires them and are
   never choices. Selections for libraries or unknown ids are ignored.
6. Choices follow the item id: when an item's file gets a new name, the new file is downloaded
   and the old one (recorded with the same `item`) is deleted, which counts as one update.
   Choices of items that left the pack are kept and apply again if the item comes back.

Manual changes. For an item whose file the last sync installed (a record exists), a sync
without an explicit choice for the item looks at the disk first:

| Record | On disk | Adopted |
| --- | --- | --- |
| enabled | file at its path | nothing |
| enabled | only `<path>.disabled` | switched off (`manual: disabled`); the disabled copy is kept and recorded with `disabled: true` |
| enabled | neither | switched off (`manual: removed`); nothing is downloaded |
| `disabled: true` | file at its path | switched on (`manual: enabled`) |
| `disabled: true` | only `<path>.disabled` | nothing (stays off, copy kept) |

An item switched off this way still comes back when an enabled item requires it (a deleted file
is downloaded, a disabled copy with the pack's hash is renamed back) and the mod center says
which items need it. When the mods folder is gone, or at least three pack mods and more than
half of them disappeared at once, the folder counts as wiped instead: nothing is adopted and
the selection is installed again (the client says so). A jar the player replaced is never
overwritten: under `mods/` an `overwrite` policy acts like `preserveUserChanges`, the record
keeps the hash we installed and gets `modified: true`, and the mod center offers to bring the
pack's file back (a selection's `revert` list: those items are planned like a repair). Jars the
player added are never touched and are listed as the player's own mods.

For every file the client should have (not `unsupported`; items: installed per the rules
above; other files: not in a disabled group):

| On disk | Policy | Action |
| --- | --- | --- |
| missing, `<path>.disabled` exists, not an item | any | keep disabled |
| missing, `<path>.disabled` exists with the manifest's SHA-1, item | any | rename back |
| missing, `<path>.disabled` exists with another SHA-1, item | any | download; delete the disabled copy if it has the recorded SHA-1 |
| missing | any | download |
| same SHA-1 as the manifest | any | unchanged |
| different | `overwrite` (not under `mods/`) | download (replace) |
| different | `ifMissing` | keep |
| different, equals the SHA-1 we recorded | `preserveUserChanges`, `overwrite` under `mods/` | download (update) |
| different, changed by the player or never ours | `preserveUserChanges`, `overwrite` under `mods/` | keep |
| unreadable, or a folder where the file belongs | any | keep (reported as an issue) |

Files recorded in the state but no longer wanted (removed from the manifest, item switched off,
group disabled, now `unsupported`) are deleted only while their SHA-1 still equals the recorded
one (also as `<path>.disabled`); changed files are kept. An item that is still in the pack but
off keeps the disabled copy the player made of it. Files that never appeared in the state (mods
the player added) are never touched. "Repair" treats every policy as `overwrite`. Downloads
happen first (verified, atomic); renames, deletions and the new state follow only when all
downloads succeeded. A rename or deletion that fails (the file is open somewhere) does not stop
the sync: the file stays, its record is kept and the next sync tries again. Files that cannot be
read never stop a sync or a launch either; they are left alone and reported. A plan that changes
no file but changes the state (new choices, new version, adopted changes) is still applied, so
choices are never lost.

Dry run ("Uygula"): planning with the pending selection gives the counts per category to add,
update and remove, the bytes to download and the bytes to delete; nothing changes on disk
until the plan is applied. An item's old file replaced by its new one counts as an update.

### Per-version sets

Every instance (the official pack's and any other) keeps one set per Minecraft version: its
`mods/` and `config/` folders and, for pack instances, its pack state. When the player moves
the instance to another version and later comes back, that version's mods, mod settings and
choices return exactly. Resource packs, shader packs, `options.txt`, worlds, `servers.dat` and
screenshots stay shared by all versions. Layout inside the game directory:

```text
.projects/set.json                               the active set's metadata
.projects/pack-state.json                        the active set's pack state
.projects/versions/<minecraft>/set.json          a parked set's metadata
.projects/versions/<minecraft>/pack-state.json   its pack state (pack instances)
.projects/versions/<minecraft>/mods/             its mods folder
.projects/versions/<minecraft>/config/           its config folder
.projects/version-switch.json                    the journal of a switch in progress
```

`<minecraft>` keeps `[A-Za-z0-9._-]`; other characters become `_` (names the path rules
refuse are hex-encoded with a `_` prefix). `set.json` (`minecraft`, `loader`, `loaderVersion`,
`packId`, `lastPlayedAt`, `parkedAt`, `sizeBytes`, `seeded`; all but `minecraft` optional)
records when the set was last played and how big it was when parked, for cleanup suggestions.
A set folder with `seeded: true` was prepared before the first visit (the player moved own mods
into it).

Moving from version A to version B:

1. Refused while the game uses the folders. A journal is written before anything moves; each
   step below is recorded before it runs and is safe to repeat, so a switch interrupted by a
   crash or a power cut is finished on the next start. Nothing is deleted on the way: a folder
   in the way is moved aside into A's parked folder.
2. Split the pack state: A's parked state keeps A's records under `mods/` and `config/` and
   A's choices. Returning to B, the new active state takes B's records under those folders, A's
   records elsewhere (those files stay in place) and B's choices. On the first visit the choices
   carry over (`items`, `seenItems`, `groups`): items B's pack offers that A's set never saw are
   new and stay off unless an enabled item requires them; the records under `config/` carry
   over too, because `config/` is copied.
3. Park: move A's `mods/` and `config/` into A's parked folder.
4. Restore: returning, move B's parked folders back. On the first visit, copy A's `config/`
   (through a temporary folder) and start an empty `mods/`, or move in the mods prepared for B.
5. Write A's parked state and metadata (with its size), the new active state and metadata, and
   remove B's parked files. The client then records the instance's new version and loader and
   removes the journal.
6. Sync B's manifest as usual: shared files that A installed and B does not want are deleted
   while unchanged; B's missing files are downloaded, and a copied config that is still the one
   A's pack installed is updated to B's.

## Mod sets

Ready-made sets ("Hazır setler") are curated lists of Modrinth mods, resource packs and shader
packs that the team keeps in Studio, for example for PvP or for performance. A player starts a
new instance from a set or adds a set to an existing instance; everything a set brings stays
optional and follows the mod center rules. The index references the catalog as `modSets`
(`modsets/modsets.<sha256>.json`).

<!-- example: ModSetCatalog -->
```json
{
  "schema": 1,
  "sets": [
    {
      "id": "pvp",
      "name": "PvP",
      "summary": "Düşük ateş, net efekt süreleri ve açlık göstergesi; sunucu kurallarına uygun.",
      "purpose": "pvp",
      "en": { "summary": "Low fire, clear effect timers and a hunger readout; within the server rules." },
      "variants": [
        {
          "dependencies": { "minecraft": "26.3", "fabric-loader": "0.19.5" },
          "files": [
            {
              "path": "mods/fabric-api-0.164.0+26.3.jar",
              "hashes": {
                "sha1": "7ec45ba92a286ed7d0c57bcd74cf9182789741ec",
                "sha512": "2ce7b0826f75801a56e8150eb5ceca467cdf8b4b44e160b5a5381e5412567d024f46a16c679f29c4a9346b2687fa1740f91ed6026a31b2ed1f3d4b5035968d69"
              },
              "downloads": ["https://cdn.modrinth.com/data/P7dR8mSH/versions/LEklt8GT/fabric-api-0.164.0%2B26.3.jar"],
              "fileSize": 1775592
            },
            {
              "path": "mods/appleskin-fabric-mc26.3-3.0.10.jar",
              "hashes": {
                "sha1": "3d19e40c5f97186857db98e622c01c2cd0030497",
                "sha512": "17d257af419b7530aa8617c2e8c0e4bbfabcf888f26655790e0188e7957e0bcd931f85f9a4c69fd27e889dee5082cf7b2b8741a14a6ed4425e9ef4bfe13a8b8c"
              },
              "downloads": ["https://cdn.modrinth.com/data/EsAfCjCV/versions/PHjDtQay/appleskin-fabric-mc26.3-3.0.10.jar"],
              "fileSize": 180194
            },
            {
              "path": "mods/statuseffecttimer-2.0.0+26.3.jar",
              "hashes": {
                "sha1": "a3447fee0a1fc1401e580bc727a1ac617951c181",
                "sha512": "7dc22d4295a6733eb7d2578e42d490c2d475c1632ff6d5a6b5a9640cca762a42db8035b0f11e9feec770e9d13e8e60a30debfe5127bd8e2a02f65272d669be20"
              },
              "downloads": ["https://cdn.modrinth.com/data/T9FDHbY5/versions/NfHPutHW/statuseffecttimer-2.0.0%2B26.3.jar"],
              "fileSize": 14952
            },
            {
              "path": "resourcepacks/PvP Basics 2.3.1.zip",
              "hashes": {
                "sha1": "483e930a8a2dbe9e2d89e24300540a452501e8dc",
                "sha512": "84ed3d15cb43d1bffc5fdfb5fcfb70c1bfe4c73ec1c68dee6b922c3dc4adab992aeb20955cdcc656633306a97736594009380d25c8e20d1f92374c48d7b50e24"
              },
              "downloads": ["https://cdn.modrinth.com/data/ehZrmfH1/versions/2tR7gDAe/PvP%20Basics%202.3.1.zip"],
              "fileSize": 410264
            }
          ],
          "items": [
            {
              "id": "P7dR8mSH",
              "path": "mods/fabric-api-0.164.0+26.3.jar",
              "title": "Fabric API",
              "category": "library",
              "modrinth": { "projectId": "P7dR8mSH", "versionId": "LEklt8GT" },
              "library": true
            },
            {
              "id": "EsAfCjCV",
              "path": "mods/appleskin-fabric-mc26.3-3.0.10.jar",
              "title": "AppleSkin",
              "summary": "Açlığı ve doygunluğu gösterir.",
              "category": "interface",
              "pageUrl": "https://modrinth.com/project/EsAfCjCV",
              "modrinth": { "projectId": "EsAfCjCV", "versionId": "PHjDtQay" },
              "requires": ["P7dR8mSH"]
            },
            {
              "id": "T9FDHbY5",
              "path": "mods/statuseffecttimer-2.0.0+26.3.jar",
              "title": "Status Effect Timer",
              "summary": "Efektlerin kalan süresi simgelerin üstünde.",
              "category": "interface",
              "modrinth": { "projectId": "T9FDHbY5", "versionId": "NfHPutHW" }
            },
            {
              "id": "ehZrmfH1",
              "path": "resourcepacks/PvP Basics 2.3.1.zip",
              "title": "PvP Basics",
              "summary": "Alçak ateş, küçük kalkan ve totem, daha az duman.",
              "category": "visual",
              "modrinth": { "projectId": "ehZrmfH1", "versionId": "2tR7gDAe" }
            }
          ]
        }
      ]
    }
  ]
}
```

- `schema` (default 1): readers refuse a catalog whose schema is higher than they support.
- `sets[]`: at most 32, in the order the launcher shows them.
  - `id`: stable, 1–64 characters from `[A-Za-z0-9._-]`, unique. Instances remember a set by it.
  - `name` (1–32 characters) and `summary` (optional, at most 140 characters, no line breaks):
    Turkish. `en` (optional) `{ name?, summary? }` is the English wording; a missing field falls
    back to the Turkish one.
  - `purpose`: `pvp | performance | visual | other` (unknown values: `other`). The launcher draws
    the card's mark from it.
  - `minLauncherVersion` (optional): older launchers do not offer the set.
  - `variants[]` (1–16): one per Minecraft version and loader.
    - `dependencies`: `minecraft` plus at most one loader, `.mrpack` spelling (`fabric-loader`,
      `quilt-loader`, `forge`, `neoforge`). The loader version is what a new instance started
      from the set gets; adding the set to an instance only needs the same loader. A variant
      without a loader holds only resource and shader packs and fits every instance on that
      version, vanilla ones too. No two variants share the Minecraft version and the loader.
    - `files[]`: `.mrpack` file entries ([Modpacks](#modpacks)), each a single file directly in
      `mods/` (`.jar`), `resourcepacks/` or `shaderpacks/` (`.zip`), with HTTPS downloads and a
      size. Studio writes only Modrinth files (`cdn.modrinth.com`): we never host set files
      (many authors' licenses forbid copies).
    - `items[]` (1–64): pack items with the rules of `projects.items` (unique ids, one file each,
      a title, HTTPS icon and page, `requires` without cycles, `library`, `defaultEnabled`), each
      with a `modrinth` reference; every file is installed by exactly one item. An item's kind
      follows its file's folder: mod, resource pack or shader pack.
- Studio refuses to publish a catalog that breaks a rule. A client leaves out the sets that break
  one (logged) and keeps the others.

What a set does in an instance:

- **Variant.** An instance on Minecraft version V with loader L uses the set's variant for V and
  L, else its loader-less variant for V. Other instances are not offered the set (the launcher
  says which version and loader it is made for). A new instance started from a set gets the
  variant's version and loader (and loader version).
- **Installing is a pack sync.** The variant becomes a pack manifest (its files and items, pack
  id = set id, `versionId` = the content key below, `configPolicy` `preserveUserChanges`) and is
  installed with the [sync semantics](#sync-semantics-and-pack-state): a set added for the first
  time selects its `defaultEnabled` items and what they require; afterwards the choices are the
  player's; items a newer catalog adds are new and stay off; items it drops are removed while
  unchanged; changes the player makes in the folders are adopted; a file the player changed is
  never overwritten.
- **Projects the instance already has are left out.** Items whose Modrinth project (or item id)
  another part of the instance already brings are not installed by the set there: every item
  the official pack offers (on or off: switching it on later must not give two copies) and the
  player's own Modrinth files. Requirements on them are dropped with them; the mod center shows
  them as present.
- **Nothing happens on its own.** Sets change files only when the player acts: add, apply
  choices, update, remove. Launches never sync sets. When the catalog's variant has other
  content than the one installed, the mod center offers the update; the player applies it or
  not.
- **Content key.** The first 16 hex digits of the SHA-256 of the variant's loader and, per item,
  its id, file path, file SHA-1, sorted requirements and library flag, sorted. Titles,
  summaries and icons are not part of it, so rewording an item is not an update.
- **Removing a set.** Its files go while unchanged; files the player changed stay as the
  player's own.
- **Resource packs** are shared by every version of the instance. A pack the set installs is
  turned on once in `options.txt` `resourcePacks`: the set's new packs go on top of the list
  (highest priority) in the set's order, its first pack topmost; the player's packs keep their
  order under them. When `options.txt` does not exist yet (a new instance), the launcher turns
  them on right after it creates the file before the first launch. A pack the player later turns
  off or moves is never touched again; a pack the set removes is taken out of the list.
- **Shader packs** are shared too. When a set puts a shader pack in place for the first time and
  Iris has none chosen (`config/iris.properties` without a `shaderPack`), the launcher chooses it
  once (`shaderPack=<file>`, `enableShaders=true`); otherwise the player picks it in Iris.
- **Old versions.** Sets carry Modrinth files only, for the versions the server supports with
  Fabric or NeoForge. 1.8.9 with Forge and OptiFine is out of scope: OptiFine may not be
  redistributed and has no Modrinth files.

The launcher records the sets of a game directory in `<game dir>/.projects/mod-sets.json`, per
Minecraft version like the pack state (parked as `.projects/versions/<minecraft>/mod-sets.json`):

<!-- example: ModSetStates -->
```json
{
  "sets": {
    "pvp": {
      "name": "PvP",
      "purpose": "pvp",
      "applied": {
        "dependencies": { "minecraft": "26.3" },
        "files": [
          {
            "path": "resourcepacks/PvP Basics 2.3.1.zip",
            "hashes": { "sha1": "483e930a8a2dbe9e2d89e24300540a452501e8dc" },
            "downloads": ["https://cdn.modrinth.com/data/ehZrmfH1/versions/2tR7gDAe/PvP%20Basics%202.3.1.zip"],
            "fileSize": 410264
          }
        ],
        "items": [
          {
            "id": "ehZrmfH1",
            "path": "resourcepacks/PvP Basics 2.3.1.zip",
            "title": "PvP Basics",
            "category": "visual",
            "modrinth": { "projectId": "ehZrmfH1", "versionId": "2tR7gDAe" }
          }
        ]
      },
      "state": {
        "packId": "pvp",
        "versionId": "3f2b7c9d1e0a4b5c",
        "minecraft": "26.3",
        "files": {
          "resourcepacks/PvP Basics 2.3.1.zip": { "sha1": "483e930a8a2dbe9e2d89e24300540a452501e8dc", "policy": "preserveUserChanges", "item": "ehZrmfH1" }
        },
        "groups": {},
        "items": { "ehZrmfH1": true },
        "seenItems": ["ehZrmfH1"]
      },
      "resourcePacks": { "PvP Basics 2.3.1.zip": true },
      "addedAt": "2026-10-19T18:00:00Z"
    }
  }
}
```

- `sets`: set id → `{ name, purpose, applied?, state, resourcePacks?, addedAt }`.
  - `name`, `purpose`: as last applied (shown when the catalog no longer has the set).
  - `applied`: the variant as last installed for this version. Missing while the set waits to
    be installed for the version (a version switch carried it over).
  - `state`: a [pack state](#sync-semantics-and-pack-state) (`packId` = set id, `versionId` =
    the applied content key, empty while not installed).
  - `resourcePacks`: file name → `true` once turned on in `options.txt`, `false` while that
    waits for the game's first `options.txt`.
- Version switches ([Per-version sets](#per-version-sets)) split every set's state like the
  pack state: the parked file keeps the leaving version's records under `mods/` and `config/`,
  its choices and its applied variant. Returning to a version that had the set brings that back
  with the current records of shared files (resource and shader packs stay in place). On a
  version that never had the set it comes along with its choices and shared records but is not
  installed: the mod center offers to install it for that version when the set has a variant
  for it.

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
- Writers: CI writes `beta.json` for every release tag and `stable.json` on a promotion, on the
  primary mirror only. Studio writes a channel file only when the admin changed that channel since
  the live state Studio last read, and only while the primary still holds exactly that state; a
  later mirror may be moved forward but never to an older version than it offers. A publish that
  would break this writes nothing anywhere. A channel removed from Studio's draft is never deleted
  live.

## HTTP caching

| Objects | `Cache-Control` |
| --- | --- |
| `index.json`, `index.json.sig`, `releases/<channel>.json` | `public, max-age=60` |
| `config/*`, `news/*`, `modsets/*`, `media/*`, `files/*`, `packs/*/*.json`, `afk/*/*`, `releases/<version>/*` | `public, max-age=31536000, immutable` |
| Feed mirrors: `feed.json`, `feed.json.sig` | `public, max-age=60` |
| Feed mirrors: `media/*` | `public, max-age=31536000, immutable` |

JSON is served as `application/json; charset=utf-8`. Mirrors must return an `ETag` for
`index.json` (feed mirrors: `feed.json`) and honour `If-None-Match` (clients send it and treat
`304` as "unchanged"). Every mirror must ignore unknown query strings (`?t=`).
GitHub's raw mirror uses its own headers (about 5 minutes), which only delays updates while the
primary is down.

## Publish order

1. Upload new immutable files: media, `files/<sha1>`, AFK builds, release bundles.
2. Upload the documents: `config/…`, `news/…`, `packs/<id>/<version>.json`, `modsets/…`.
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

The news feed has its own cache in `<data>/cache/feed/`, refreshed right after the content index
(no polling of its own) and deleted when the config disables the feed:

```text
current.json            exact feed.json text + signature, ETags per mirror, fetch time
media/<sha256>.<ext>    covers of the current and the previous feed (verified on download)
```
