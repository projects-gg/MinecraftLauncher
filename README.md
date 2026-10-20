<div align="center">
<a href="https://projects.gg"><img src="https://projects.gg/medya/ProjectsLogo.png" alt="projects.gg"></a>

## Projects Launcher

[![GNU](https://img.shields.io/github/license/projects-gg/MinecraftLauncher?&logo=github)](LICENSE)
[![Join us on Discord (Turkish)](https://img.shields.io/discord/480094926164590593.svg?label=&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2)](https://projects.gg/discord)

The Minecraft launcher of [projects.gg](https://projects.gg): every Minecraft version and mod
loader, the server's official modpack kept up to date automatically, news on start, AFK accounts,
and offline play when the internet or our servers are down. Windows, macOS and Linux.

</div>

## Contact

Join us on [Discord](https://projects.gg/discord) (Turkish).

## Downloads

The latest version is on the [Projects landing page](https://projects.gg/indir/). Installed
launchers update themselves in the background.

## What's inside (v2)

- **Instances like MultiMC/Prism:** any Minecraft version (year-based 26.x included), Fabric,
  Quilt, Forge and NeoForge, per-instance Java, memory, JVM arguments, window and hooks; mods,
  resource packs, shaders, worlds, screenshots and servers per instance; Modrinth browser with
  dependency resolution and updates; `.mrpack`, Prism/MultiMC and CurseForge import, `.mrpack`
  export.
- **The official pack:** the "Projects" instance follows the pack published from Projects
  Studio; files sync before each launch without touching anything the player added.
- **Home:** season art, news, patch notes and event countdowns published without an app update;
  maintenance notices; the big gold OYNA button.
- **Accounts:** Microsoft sign-in (browser + PKCE, tokens in the OS keychain) and offline names;
  skins and capes for Microsoft accounts.
- **AFK accounts** through the Projects Console Client, with the full v1 settings.
- **Offline first:** cached signed content and installed files are enough to play.
- **Updates:** signed, channel-based (stable/beta), staged rollout, mandatory minimum version,
  "What's new" after an update.
- **Desktop:** tray icon, start with the computer, `projects://` links, Ctrl+K command palette,
  modern Discord Rich Presence, crash assistant.
- **Moving from v1:** nickname, premium session, AFK accounts, worlds, screenshots and packs are
  imported on first start.

## Repository

```
apps/launcher     Tauri 2 app: React UI (src) + Rust shell (src-tauri)
crates/pl-net     HTTP + parallel verified downloads
crates/pl-mc      Minecraft: versions, install, Java, launch, instances, loaders, mods, Modrinth, modpacks
crates/pl-auth    Microsoft + offline accounts, keychain secrets, skins
crates/pl-content Signed content index, news/home models, modpack sync (shared with Studio)
crates/pl-afk     AFK accounts (Projects Console Client)
crates/pl-discord Discord Rich Presence
crates/pl-data    Data folder: usage, cleanup policy, support bundles, logs, moving the folder
crates/pl-desktop Desktop integration: projects:// links, desktop shortcuts and their icons
crates/pl-social  Client for the optional friends service (HTTP API, presence WebSocket)
packages/ui       Shared design system
docs/             Content format specification
```

Projects Studio (the team's publishing tool) and the online services the launcher talks to (the
friends service, the Studio team gate) are developed in a separate repository that uses this one
as a submodule. Nothing from it is built into the launcher.

The signed content tree that Studio publishes and the launcher reads is specified in
[`docs/content-format.md`](docs/content-format.md); its JSON examples are checked by the tests.

Conventions: crates never depend on Tauri; the app crate exposes typed commands
(tauri-specta generates `apps/launcher/src/bindings.ts`, regenerate with
`cargo run -p projects-launcher --features export-bindings --bin export-bindings`; the feature keeps
the tool out of the installers). Runtime secrets live in the OS keychain,
never in files or in the repository. User-facing text is Turkish; code, comments, commit messages
and developer documentation are English.

## Building

Requirements: Rust stable (see `rust-toolchain.toml`), Node 22+, pnpm 10, and the
[Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS.

```sh
pnpm install
pnpm --filter @pl/launcher tauri dev     # launcher with hot reload
pnpm --filter @pl/launcher dev           # UI only, in a browser with mock data
cargo test --workspace                   # Rust tests
pnpm lint && pnpm typecheck              # Biome + TypeScript
```

Releases are built by GitHub Actions (`.github/workflows/release.yml`) from a `v*` tag: signed
updater bundles for Windows, macOS and Linux, a draft GitHub release and, when the R2 secrets are
configured, the update channel files on the CDN. A local installer needs the updater signing key
in `TAURI_SIGNING_PRIVATE_KEY`:

```sh
pnpm --filter @pl/launcher tauri build --bundles nsis
```

### Microsoft sign-in in forks

Forks need their own Azure application. Register one at [portal.azure.com](https://portal.azure.com)
→ *App registrations* → *New registration*: personal Microsoft accounts only, redirect URI
`http://localhost` (public client), "Allow public client flows" on. Put its client id into
`CLIENT_ID` in `crates/pl-auth/src/config.rs`. Client ids are public values, not secrets.

## License

All commits are licensed under the GNU license, unless otherwise noted in the patch headers.
