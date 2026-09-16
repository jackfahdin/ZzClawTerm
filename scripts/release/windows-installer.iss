; ZzClawTerm Windows installer (Inno Setup 6)
;
; Built by scripts/release/package_native.py, which stages the payload into a
; directory and passes it here through iscc /D defines. The defaults below
; only exist so the script can be compiled by hand while iterating.
;
; Per-user install: no admin rights, everything lands under
; %LOCALAPPDATA%\Programs\ZzClawTerm and HKCU.

#ifndef SourceDir
  #define SourceDir "..\..\dist-work\windows-installer"
#endif
#ifndef Version
  #define Version "0.0.1"
#endif
#ifndef NumericVersion
  #define NumericVersion "0.0.1.0"
#endif
#ifndef OutputDir
  #define OutputDir "..\..\dist"
#endif
#ifndef OutputName
  #define OutputName "ZzClawTerm-setup"
#endif
#ifndef Arch
  #define Arch "x64"
#endif

[Setup]
AppId={{8F2A6E3D-4C5B-4A1F-9E7D-2B6C8A0D1F3E}
AppName=ZzClawTerm
AppVersion={#Version}
AppPublisher=Jackfahdin
AppPublisherURL=https://github.com/jackfahdin/ZzClawTerm
AppSupportURL=https://github.com/jackfahdin/ZzClawTerm/issues
VersionInfoVersion={#NumericVersion}
VersionInfoDescription=ZzClawTerm native GPUI terminal
VersionInfoCopyright=Copyright Jackfahdin
DefaultDirName={localappdata}\Programs\ZzClawTerm
DefaultGroupName=ZzClawTerm
PrivilegesRequired=lowest
OutputDir={#OutputDir}
OutputBaseFilename={#OutputName}
SetupIconFile={#SourceDir}\icon.ico
UninstallDisplayIcon={app}\ZzClawTerm.exe
Compression=lzma2/ultra64
SolidCompression=yes
WizardStyle=modern
#if Arch == "arm64"
ArchitecturesAllowed=arm64
ArchitecturesInstallIn64BitMode=arm64
#else
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
#endif

[Languages]
Name: "english"; MessagesFile: "compiler:Default.isl"

[Files]
; Every staged executable: the application plus all helper binaries. The
; staging directory is created fresh per package run, so the wildcard cannot
; pick up strays.
Source: "{#SourceDir}\*.exe"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\LICENSE"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\VERSION"; DestDir: "{app}"; Flags: ignoreversion
Source: "{#SourceDir}\icon.ico"; DestDir: "{app}"; Flags: ignoreversion
; VcXsrv is optional: package_native.py stages it when a distribution is
; available and skips it (with a warning) otherwise.
#if FileExists(SourceDir + "\vcxsrv\vcxsrv.exe")
Source: "{#SourceDir}\vcxsrv\*"; DestDir: "{app}\vcxsrv"; Flags: recursesubdirs createallsubdirs ignoreversion
#endif

[Icons]
Name: "{group}\ZzClawTerm"; Filename: "{app}\ZzClawTerm.exe"; IconFilename: "{app}\icon.ico"
Name: "{autodesktop}\ZzClawTerm"; Filename: "{app}\ZzClawTerm.exe"; IconFilename: "{app}\icon.ico"

[Registry]
; Install location record, kept for compatibility with installs made by the
; previous NSIS installer.
Root: HKCU; Subkey: "Software\ZzClawTerm"; ValueType: string; ValueName: "InstallDir"; ValueData: "{app}"; Flags: uninsdeletekey
; zzclawterm:// URL scheme
Root: HKCU; Subkey: "Software\Classes\zzclawterm"; ValueType: string; ValueData: "URL:ZzClawTerm Protocol"; Flags: uninsdeletekey
Root: HKCU; Subkey: "Software\Classes\zzclawterm"; ValueType: string; ValueName: "URL Protocol"; ValueData: ""
Root: HKCU; Subkey: "Software\Classes\zzclawterm\DefaultIcon"; ValueType: string; ValueData: "{app}\ZzClawTerm.exe,0"
Root: HKCU; Subkey: "Software\Classes\zzclawterm\shell\open\command"; ValueType: string; ValueData: """{app}\ZzClawTerm.exe"" ""%1"""
