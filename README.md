<div align="center">
<a href="https://projects.gg"><img src="https://projects.gg/medya/ProjectsLogo.png" alt="projects.gg"></a>

## Projects Launcher

[![GNU](https://img.shields.io/github/license/projects-gg/MinecraftLauncher?&logo=github)](LICENSE)
[![Codacy Badge](https://app.codacy.com/project/badge/Grade/17980a84919b4fff967e047ba0487517)](https://www.codacy.com/gh/projects-gg/MinecraftLauncher/dashboard?utm_source=github.com&amp;utm_medium=referral&amp;utm_content=projects-gg/MinecraftLauncher&amp;utm_campaign=Badge_Grade)
[![Join us on Discord (Turkish)](https://img.shields.io/discord/480094926164590593.svg?label=&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2)](https://projects.gg/discord)

[![Projects Launcher's Stars](https://img.shields.io/github/stars/projects-gg/minecraftlauncher?label=stars&logo=github)](https://github.com/projects-gg/minecraftlauncher/stargazers)
[![Projects Launcher's Forks](https://img.shields.io/github/forks/projects-gg/minecraftlauncher?label=forks&logo=github)](https://github.com/projects-gg/minecraftlauncher/network/members)

Projects Launcher is a Minecraft Launcher that created to make starting the game easier and faster with QoL features.

<a href="https://projects.gg"><img src="https://projects.gg/medya/launcher.png" alt="projects.gg"></a>

</div>

## Contact
[![Join us on Discord](https://img.shields.io/discord/480094926164590593.svg?label=&logo=discord&logoColor=ffffff&color=7389D8&labelColor=6A7EC2)](https://projects.gg/discord)

Join us on [Discord](https://projects.gg/discord) (Turkish)

## Downloads
Latest version can be obtained from [Projects Landing Page](https://projects.gg/indir/).
[VirusTotal Results](https://projects.gg/launchervirustotal).

## License
[![GNU License](https://img.shields.io/github/license/projects-gg/MinecraftLauncher?&logo=github)](LICENSE)

All commits are licensed under the GNU license, unless otherwise noted in the patch headers.

## Building and setting up

### Initial setup
Open the project in any C# IDE like Visual Studio. Project is based on .NET 4.7.2.

### Compiling
Open `Projects Launcher.sln` file with any C# IDE and simply, click to `Build -> Build Solution` above of the page. Output will save to `./Debug/` folder.

### Premium (Microsoft account) sign-in
The launcher can start the game with a real, purchased Minecraft account so players join servers in
online mode. The whole flow lives in `Projects Launcher/Auth/` and has no extra NuGet dependency:

| File | Responsibility |
| --- | --- |
| `MicrosoftAuth.cs` | OAuth 2.0 with PKCE → Xbox Live → XSTS → Minecraft Services → profile |
| `PremiumSession.cs` | Stores the session, refreshes expired tokens, maps it to a `CmlLib` session |
| `PremiumLoginForm.cs` | The sign-in window (progress steps, license warning, errors) |
| `MicrosoftBrand.cs` | Draws the Microsoft logo so no image asset is needed |

The authorization code is captured by a short-lived loopback listener on a random free port, so the
user signs in through their own browser on Microsoft's real page — the launcher never sees the
password. The Microsoft refresh token is stored at `%APPDATA%\.projects\premium.dat`, encrypted with
DPAPI (`CurrentUser` scope), so the file is useless on another account or machine.

**Forks need their own Azure application.** Register one at
[portal.azure.com](https://portal.azure.com) → *App registrations* → *New registration*:

1. **Supported account types:** `Personal Microsoft accounts only`
2. **Redirect URI:** platform `Public client/native (mobile & desktop)`, value `http://localhost`
3. **Authentication → Allow public client flows:** `Yes`
4. Copy the *Application (client) ID* into `MicrosoftAuth.ClientId`

No API permission needs to be added in the portal; `XboxLive.signin offline_access` is requested at
sign-in time. Client IDs are public values, not secrets.
