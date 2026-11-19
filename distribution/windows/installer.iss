; Inno Setup Script for FantasTech
; Creates a standard Windows installer (.exe) for 64-bit Windows

#define MyAppName "FantasTech"
#define MyAppVersion "0.1.5"
#define MyAppPublisher "Ahmad Serieh"
#define MyAppExeName "fantastech.exe"

[Setup]
AppId={{E58F4941-C68A-4B2E-8E7D-9A65D65C1089}
AppName={#MyAppName}
AppVersion={#MyAppVersion}
AppPublisher={#MyAppPublisher}
DefaultDirName={autopf}\{#MyAppName}
DefaultGroupName={#MyAppName}
AllowNoIcons=yes
LicenseFile=..\..\LICENSE
OutputDir=..\..\dist
OutputBaseFilename=FantasTech-Setup-{#MyAppVersion}
SetupIconFile=..\..\assets\icons\fantastech.ico
Compression=lzma
SolidCompression=no
WizardStyle=modern
ArchitecturesInstallIn64BitMode=x64compatible
PrivilegesRequired=lowest
PrivilegesRequiredOverridesAllowed=commandline dialog
CloseApplications=force
CloseApplicationsFilter=*fantastech.exe*
UninstallDisplayName={#MyAppName}
UninstallDisplayIcon={app}\icons\fantastech.ico
VersionInfoVersion={#MyAppVersion}.0
VersionInfoCompany={#MyAppPublisher}
VersionInfoDescription=FantasTech MK894 Control Suite Setup
VersionInfoTextVersion={#MyAppVersion}
VersionInfoCopyright=Copyright (C) 2026 {#MyAppPublisher}
VersionInfoProductName={#MyAppName}
VersionInfoProductVersion={#MyAppVersion}.0

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Tasks]
Name: "desktopicon"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked
Name: "addtopath"; Description: "Add FantasTech CLI to user PATH environment variable"; GroupDescription: "System Integration:"
Name: "autostart"; Description: "Start FantasTech background service automatically on user login"; GroupDescription: "System Integration:"

[Files]
Source: "..\..\core\target\release\{#MyAppExeName}"; DestDir: "{app}"; Flags: ignoreversion
Source: "..\..\assets\icons\fantastech.ico"; DestDir: "{app}\icons"; Flags: ignoreversion
Source: "..\..\README.md"; DestDir: "{app}"; Flags: isreadme

[Icons]
Name: "{group}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\icons\fantastech.ico"
Name: "{group}\{cm:UninstallProgram,{#MyAppName}}"; Filename: "{uninstallexe}"
Name: "{autodesktop}\{#MyAppName}"; Filename: "{app}\{#MyAppExeName}"; IconFilename: "{app}\icons\fantastech.ico"; Tasks: desktopicon
Name: "{userstartup}\{#MyAppName} Service"; Filename: "{app}\{#MyAppExeName}"; Parameters: "service run"; IconFilename: "{app}\icons\fantastech.ico"; Tasks: autostart

[Run]
Filename: "{app}\{#MyAppExeName}"; Description: "{cm:LaunchProgram,{#StringChange(MyAppName, '&', '&&')}}"; Flags: nowait postinstall skipifsilent

[Registry]
Root: HKCU; Subkey: "Software\Microsoft\Windows\CurrentVersion\Run"; ValueType: string; ValueName: "FantasTechService"; ValueData: """{app}\{#MyAppExeName}"" service run"; Tasks: autostart; Flags: uninsdeletevalue

[UninstallDelete]
Type: filesandordirs; Name: "{app}"
Type: filesandordirs; Name: "{group}"
Type: files; Name: "{autodesktop}\{#MyAppName}.lnk"
Type: filesandordirs; Name: "{localappdata}\FantasTech"
Type: files; Name: "{userstartup}\{#MyAppName} Service.lnk"
Type: files; Name: "{userstartup}\{#MyAppName}-Service.cmd"
Type: files; Name: "{userstartup}\{#MyAppName} Service.cmd"

[Code]
// Terminate any running instance of FantasTech (GUI or service) before uninstallation begins
function InitializeUninstall(): Boolean;
var
  ResultCode: Integer;
begin
  Exec('taskkill.exe', '/F /IM fantastech.exe', '', SW_HIDE, ewWaitUntilTerminated, ResultCode);
  Sleep(400);
  Result := True;
end;

// Prompt user whether they want to purge user settings, profiles, and configuration
procedure CurUninstallStepChanged(CurUninstallStep: TUninstallStep);
var
  AppDir: String;
  ResultCode: Integer;
begin
  if CurUninstallStep = usPostUninstall then
  begin
    // Ask user whether to delete configurations and saved profiles
    if MsgBox('Do you want to delete all FantasTech configurations, saved profiles, and settings?' + #13#10#13#10 +
              'Click "Yes" to remove all user profiles and configuration data.' + #13#10 +
              'Click "No" to retain saved profiles for future reinstallation.',
              mbConfirmation, MB_YESNO or MB_DEFBUTTON2) = IDYES then
    begin
      DelTree(ExpandConstant('{userappdata}\FantasTech'), True, True, True);
      DelTree(ExpandConstant('{localappdata}\FantasTech'), True, True, True);
      DelTree(ExpandConstant('{userappdata}\fantastech'), True, True, True);
      DelTree(ExpandConstant('{localappdata}\fantastech'), True, True, True);
    end;

    // Detached cleanup: purge application directory and unins000.exe after uninstaller process exits
    AppDir := ExpandConstant('{app}');
    Exec('cmd.exe', '/c ping 127.0.0.1 -n 2 >nul & rmdir /s /q "' + AppDir + '"', '', SW_HIDE, ewNoWait, ResultCode);
  end;
end;
