; Universal Screens — Windows installer (Inno Setup 6).
;
; Build it with `scripts/build-installer.ps1`, which compiles the release
; binaries first and passes AppVersion / SourceDir / OutputDir in. Compiling
; this file on its own will fail on the missing defines — deliberately, so a
; stale hand-run can't quietly package yesterday's binaries.
;
; Deliberate choices:
;   * Per-user by default (PrivilegesRequired=lowest) — installs under
;     %LOCALAPPDATA%\Programs with no UAC prompt at all, which matters because
;     plenty of the people this is aimed at are on a locked-down work laptop.
;     `/ALLUSERS` still forces a machine-wide install for IT deployment.
;   * Unsigned. Standing UNI·SIM policy — see the SmartScreen note in the docs.
;     Windows will warn on first run; that is expected, not a broken download.
;   * No VC++ redistributable, no bundled DLLs: the binaries are built with a
;     static CRT (`-C target-feature=+crt-static`), so the install is two
;     self-contained .exe files. Don't drop that flag from the build script
;     without adding the redist back here.
;   * No firewall rule at install time. The host asks for one from its own UI
;     when it's actually needed (crates/host-windows/src/firewall.rs), which
;     costs one UAC prompt at the moment the user understands why.

#ifndef AppVersion
  #error AppVersion is not defined — build via scripts/build-installer.ps1
#endif
#ifndef SourceDir
  #error SourceDir is not defined — build via scripts/build-installer.ps1
#endif
#ifndef OutputDir
  #error OutputDir is not defined — build via scripts/build-installer.ps1
#endif

#define AppName        "Universal Screens"
#define AppPublisher   "Universal Simulation Ltd"
#define AppUrl         "https://opensource.unisim.co.uk/screens"
#define AppRepo        "https://github.com/universal-simulation-ltd/Universal_Screens"
#define HostExe        "extender-host-windows.exe"
#define ClientExe      "extender-client.exe"

[Setup]
; Never reuse this GUID in another product — it's the identity Windows upgrades on.
AppId={{B12DB31E-27A4-4502-BD5F-B32EDA901DB9}
AppName={#AppName}
AppVersion={#AppVersion}
VersionInfoVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL={#AppUrl}
AppSupportURL={#AppRepo}/issues
AppUpdatesURL={#AppRepo}/releases
DefaultDirName={autopf}\Universal Screens
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
LicenseFile={#SourceDir}\LICENSE
OutputDir={#OutputDir}
OutputBaseFilename=UniversalScreens-Setup-{#AppVersion}
SetupIconFile={#SourceDir}\crates\host-windows\assets\app-icon.ico
UninstallDisplayIcon={app}\{#HostExe}
UninstallDisplayName={#AppName}
WizardStyle=modern
Compression=lzma2/max
SolidCompression=yes
; The binaries are x86_64 — refuse ARM-emulated/32-bit installs rather than
; installing something that won't start.
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
; Allows `UniversalScreens-Setup.exe /ALLUSERS` for machine-wide deployment
; without putting an extra choice in front of an ordinary user.
PrivilegesRequiredOverridesAllowed=commandline
; Offer to shut a running host down instead of failing on a locked file.
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Types]
Name: "full";   Description: "Host and desktop client"
Name: "custom"; Description: "Choose what to install"; Flags: iscustom

[Components]
Name: "host";   Description: "Universal Screens host — share this PC's screen or let a phone drive it"; Types: full custom; Flags: fixed
Name: "client"; Description: "Desktop client — use this PC as a second screen for another computer (command line)"; Types: full custom

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\target\x86_64-pc-windows-msvc\release\{#HostExe}";   DestDir: "{app}"; Components: host;   Flags: ignoreversion
Source: "{#SourceDir}\target\x86_64-pc-windows-msvc\release\{#ClientExe}"; DestDir: "{app}"; Components: client; Flags: ignoreversion
Source: "{#SourceDir}\crates\host-windows\assets\app-icon.ico";            DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE";                                           DestDir: "{app}"; DestName: "LICENSE.txt"; Flags: ignoreversion
Source: "{#SourceDir}\installer\README-installed.txt";                    DestDir: "{app}"; DestName: "README.txt"; Flags: ignoreversion isreadme

[Icons]
; Only the host gets a shortcut: it's the GUI. The client takes a host address on
; the command line, so a bare double-click would just fail to reach 127.0.0.1 —
; README.txt explains how to run it.
Name: "{group}\{#AppName}";     Filename: "{app}\{#HostExe}"; IconFilename: "{app}\app-icon.ico"; Comment: "Share this PC's screen, or let a phone drive it"; Components: host
Name: "{group}\Read me";        Filename: "{app}\README.txt"; Components: host
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#HostExe}"; IconFilename: "{app}\app-icon.ico"; Tasks: desktopicon; Components: host

[Run]
Filename: "{app}\{#HostExe}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent; Components: host

[UninstallDelete]
; eframe's "persistence" feature remembers window state and the "don't connect
; automatically" preference here. Leave it on an upgrade, clear it on uninstall.
Type: filesandordirs; Name: "{userappdata}\Universal Screens Host"
; ...and the pre-rename directory, for anyone who ran the old build.
Type: filesandordirs; Name: "{userappdata}\Screen Extender Host"
