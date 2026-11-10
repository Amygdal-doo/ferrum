; Inno Setup script shared by every Ferrum app. scripts/package-windows.ps1
; compiles it with /D defines:
;   AppId          ferrum app id (ide, api, git, data, kit, dock, launcher)
;   AppName        display name, e.g. "FerrumData"
;   AppSemver      full version, e.g. 0.2.0-beta.1
;   AppNumericVer  four-part numeric version for the file metadata, e.g. 0.2.0.0
;   ExeName        main executable, e.g. ferrumdata.exe
;   SourceDir      folder with the files to install (exes and <AppId>.ico)
;   OutputDir      where the setup .exe is written
;   OutputName     setup file name without .exe
;
; Installs per user (no admin) into %LOCALAPPDATA%\Programs\Ferrum\<AppName>.

#ifndef AppId
  #error AppId is not defined; run scripts/package-windows.ps1
#endif

[Setup]
AppId=dev.ferrum.{#AppId}
AppName={#AppName}
AppVersion={#AppSemver}
AppVerName={#AppName} {#AppSemver}
AppPublisher=Amygdal
AppPublisherURL=https://ferrum-web-production.up.railway.app
AppSupportURL=https://ferrum-web-production.up.railway.app/download
VersionInfoVersion={#AppNumericVer}
VersionInfoProductName={#AppName}
VersionInfoProductVersion={#AppNumericVer}
VersionInfoDescription={#AppName} Setup
DefaultDirName={localappdata}\Programs\Ferrum\{#AppName}
DefaultGroupName=Ferrum
DisableProgramGroupPage=yes
DisableDirPage=auto
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}
SetupIconFile={#SourceDir}\{#AppId}.ico
UninstallDisplayIcon={app}\{#AppId}.ico
UninstallDisplayName={#AppName}
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
CloseApplications=yes

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "{#SourceDir}\*"; DestDir: "{app}"; Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\Ferrum\{#AppName}"; Filename: "{app}\{#ExeName}"; IconFilename: "{app}\{#AppId}.ico"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#ExeName}"; IconFilename: "{app}\{#AppId}.ico"; Tasks: desktopicon

[Run]
Filename: "{app}\{#ExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent
