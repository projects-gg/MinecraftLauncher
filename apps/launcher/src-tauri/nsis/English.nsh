; Projects Launcher installer: English strings.
;
; Replaces Tauri's built-in English.nsh through tauri.conf.json > bundle > windows > nsis >
; customLanguageFiles, so every string the installer template uses (nsis/installer.nsi: Tauri's
; template with the INSTALL-UX changes) is defined here. Turkish.nsh carries the same names;
; scripts/release/nsis-template.test.mjs checks both.
;
; The last block overrides one Modern UI string; redefining it gives a "set multiple times"
; warning when the installer is built, as Turkish.nsh does on purpose.
;
; {{product_name}} is replaced with the app's name during the install; keep it.

; --- Tauri's strings ---------------------------------------------------------------------------
LangString addOrReinstall ${LANG_ENGLISH} "Add/Reinstall components"
LangString alreadyInstalled ${LANG_ENGLISH} "Already Installed"
LangString alreadyInstalledLong ${LANG_ENGLISH} "${PRODUCTNAME} ${VERSION} is already installed. Select the operation you want to perform and click Next to continue."
LangString appRunning ${LANG_ENGLISH} "{{product_name}} is running! Please close it first then try again."
LangString appRunningOkKill ${LANG_ENGLISH} "{{product_name}} is running!$\nClick OK to kill it"
LangString chooseMaintenanceOption ${LANG_ENGLISH} "Choose the maintenance option to perform."
LangString choowHowToInstall ${LANG_ENGLISH} "Choose how you want to install ${PRODUCTNAME}."
LangString createDesktop ${LANG_ENGLISH} "Create desktop shortcut"
LangString dontUninstall ${LANG_ENGLISH} "Do not uninstall"
LangString dontUninstallDowngrade ${LANG_ENGLISH} "Do not uninstall (Downgrading without uninstall is disabled for this installer)"
LangString failedToKillApp ${LANG_ENGLISH} "Failed to kill {{product_name}}. Please close it first then try again"
LangString installingWebview2 ${LANG_ENGLISH} "Installing WebView2..."
LangString newerVersionInstalled ${LANG_ENGLISH} "A newer version of ${PRODUCTNAME} is already installed! It is not recommended that you install an older version. If you really want to install this older version, it's better to uninstall the current version first. Select the operation you want to perform and click Next to continue."
LangString older ${LANG_ENGLISH} "older"
LangString olderOrUnknownVersionInstalled ${LANG_ENGLISH} "An $R4 version of ${PRODUCTNAME} is installed on your system. It's recommended that you uninstall the current version before installing. Select the operation you want to perform and click Next to continue."
LangString silentDowngrades ${LANG_ENGLISH} "Downgrades are disabled for this installer, can't proceed with the silent installer, please use the graphical interface installer instead.$\n"
LangString unableToUninstall ${LANG_ENGLISH} "Unable to uninstall!"
LangString uninstallApp ${LANG_ENGLISH} "Uninstall ${PRODUCTNAME}"
LangString uninstallBeforeInstalling ${LANG_ENGLISH} "Uninstall before installing"
LangString unknown ${LANG_ENGLISH} "unknown"
LangString webview2AbortError ${LANG_ENGLISH} "Failed to install WebView2! The app can't run without it. Try restarting the installer."
LangString webview2DownloadError ${LANG_ENGLISH} "Error: Downloading WebView2 Failed - $0"
LangString webview2DownloadSuccess ${LANG_ENGLISH} "WebView2 bootstrapper downloaded successfully"
LangString webview2Downloading ${LANG_ENGLISH} "Downloading WebView2 bootstrapper..."
LangString webview2InstallError ${LANG_ENGLISH} "Error: Installing WebView2 failed with exit code $1"
LangString webview2InstallSuccess ${LANG_ENGLISH} "WebView2 installed successfully"

; --- Projects strings (INSTALL-UX: this version or a newer one is installed already) -----------
LangString projectsInstalledSub ${LANG_ENGLISH} "${PRODUCTNAME} is installed on this computer."
LangString projectsSameText ${LANG_ENGLISH} "This version is already installed. There's no need to install it again."
LangString projectsNewerText ${LANG_ENGLISH} "The version on your computer is newer than this installer. There's nothing to do."
LangString projectsReinstall ${LANG_ENGLISH} "Reinstall anyway (if the launcher doesn't open)"
LangString projectsOpenButton ${LANG_ENGLISH} "Open launcher"
LangString projectsReinstallButton ${LANG_ENGLISH} "Reinstall"

; --- Modern UI 2 page text ---------------------------------------------------------------------
LangString MUI_UNTEXT_CONFIRM_SUBTITLE ${LANG_ENGLISH} "Your accounts, settings and worlds stay on this computer."
