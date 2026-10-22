; LEGACY COMPATIBILITY PACKAGE — not used for SEL release delivery.
; Use the WiX bootstrapper generated from packaging/bundle/Bundle.wxs.
#define AppName "SEL Facturation"
#define AppVersion "0.3.26"
[Setup]
AppId={{7B258C72-AE0B-4D4E-84DE-725C08F20EA1}
AppName={#AppName}
AppVersion={#AppVersion}
DefaultDirName={localappdata}\Programs\SEL Facturation
DefaultGroupName=SEL Facturation
OutputDir=..\dist\installer
OutputBaseFilename=SEL-Facturation-Setup-0.3.26
Compression=lzma2
SolidCompression=yes
PrivilegesRequired=lowest
UninstallDisplayIcon={app}\SEL-Facturation.exe
WizardStyle=modern
[Files]
Source: "..\dist\SEL-Facturation.exe"; DestDir: "{app}"; Flags: ignoreversion
[Icons]
Name: "{autoprograms}\SEL Facturation"; Filename: "{app}\SEL-Facturation.exe"
Name: "{autodesktop}\SEL Facturation"; Filename: "{app}\SEL-Facturation.exe"; Tasks: desktopicon
[Tasks]
Name: "desktopicon"; Description: "Ajouter un raccourci Bureau"; GroupDescription: "Raccourcis :"; Flags: checkedonce
[Run]
Filename: "{app}\SEL-Facturation.exe"; Description: "Lancer SEL Facturation"; Flags: nowait postinstall skipifsilent
