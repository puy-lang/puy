// TOSVersion (docs/spec/stdlib/System.SysUtils/TOSVersion.md). The host reports the kind of OS
// and the architecture in the spelling of Rust's std::env::consts (System.SysUtils.Native).

{ TOSVersion }

class procedure TOSVersion.Load;
begin
  if FLoaded then
    Exit;
  var V := OsVersion;
  if V.Platform = 'windows' then
    FPlatform := TPlatform.pfWindows
  else if V.Platform = 'macos' then
    FPlatform := TPlatform.pfMacOS
  else if V.Platform = 'ios' then
    FPlatform := TPlatform.pfiOS
  else if V.Platform = 'android' then
    FPlatform := TPlatform.pfAndroid
  else if V.Platform = 'linux' then
    FPlatform := TPlatform.pfLinux
  else if V.Platform = 'wasi' then
    FPlatform := TPlatform.pfWasi
  else if V.Platform = 'browser' then
    FPlatform := TPlatform.pfBrowser
  else
    FPlatform := TPlatform.pfEmbedded;
  if V.Arch = 'x86' then
    FArchitecture := TArchitecture.arIntelX86
  else if V.Arch = 'x86_64' then
    FArchitecture := TArchitecture.arIntelX64
  else if V.Arch = 'arm' then
    FArchitecture := TArchitecture.arARM32
  else if V.Arch = 'aarch64' then
    FArchitecture := TArchitecture.arARM64
  else if V.Arch = 'wasm32' then
    FArchitecture := TArchitecture.arWasm32
  else
    FArchitecture := TArchitecture.arOther;
  FArchName := V.Arch;
  FName := V.Name;
  FMajor := V.Major;
  FMinor := V.Minor;
  FBuild := V.Build;
  FLoaded := True;
end;

class function TOSVersion.GetArchitecture: TArchitecture;
begin
  Load;
  Result := FArchitecture;
end;

class function TOSVersion.GetPlatform: TPlatform;
begin
  Load;
  Result := FPlatform;
end;

class function TOSVersion.GetName: string;
begin
  Load;
  Result := FName;
end;

class function TOSVersion.GetMajor: Integer;
begin
  Load;
  Result := FMajor;
end;

class function TOSVersion.GetMinor: Integer;
begin
  Load;
  Result := FMinor;
end;

class function TOSVersion.GetBuild: Integer;
begin
  Load;
  Result := FBuild;
end;

class function TOSVersion.GetServicePackMajor: Integer;
begin
  Result := 0;
end;

class function TOSVersion.GetServicePackMinor: Integer;
begin
  Result := 0;
end;

class function TOSVersion.Check(AMajor: Integer): Boolean;
begin
  Result := Major >= AMajor;
end;

class function TOSVersion.Check(AMajor, AMinor: Integer): Boolean;
begin
  Result := (Major > AMajor) or ((Major = AMajor) and (Minor >= AMinor));
end;

class function TOSVersion.Check(AMajor, AMinor, AServicePackMajor: Integer): Boolean;
begin
  // There are no service packs (ServicePackMajor is 0)
  Result := (Major > AMajor) or ((Major = AMajor) and (Minor > AMinor))
    or ((Major = AMajor) and (Minor = AMinor) and (AServicePackMajor <= 0));
end;

class function TOSVersion.ToString: string;
begin
  Load;
  Result := Format('%s %d.%d.%d (%s)', [FName, FMajor, FMinor, FBuild, FArchName]);
end;
