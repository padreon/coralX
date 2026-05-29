; coralX Inno Setup script
; Run from repo root: iscc /DAppVersion=1.2.3 installer\windows.iss
; Output: coralX-windows-setup.exe  (in repo root)

#ifndef AppVersion
  #define AppVersion "0.0.0-dev"
#endif

#define AppName    "coralX"
#define AppExeName "coralX.exe"
#define AppURL     "https://github.com/padreon/coralX"

[Setup]
AppId={{E3C2A1B0-4D5E-6F70-8901-2A3B4C5D6E7F}
AppName={#AppName}
AppVersion={#AppVersion}
AppPublisher=coralX
AppPublisherURL={#AppURL}
AppSupportURL={#AppURL}
AppUpdatesURL={#AppURL}
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
; SourceDir = repo root (one level up from this file)
; OutputDir = repo root — same as SourceDir
SourceDir=..
OutputDir=..
OutputBaseFilename=coralX-windows-setup
SetupIconFile=assets\icon.ico
Compression=lzma2
SolidCompression=yes
WizardStyle=modern
PrivilegesRequired=lowest
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; \
    GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[Files]
Source: "dist\coralX\*"; DestDir: "{app}"; \
    Flags: ignoreversion recursesubdirs createallsubdirs

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; \
    Tasks: desktopicon

[Run]
Filename: "{app}\{#AppExeName}"; \
    Description: "{cm:LaunchProgram,{#StringChange(AppName, '&', '&&')}}"; \
    Flags: nowait postinstall skipifsilent

[UninstallDelete]
Type: dirifempty; Name: "{app}"
