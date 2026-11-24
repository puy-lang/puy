unit Puy.Platform;

// Which capabilities of the host the program may use (docs/spec/stdlib/Puy.Platform/README.md).
// The answer is what the isolate's HostSet holds (docs/design/stdlib.md 3.4.2): a capability is
// there, or missing because the target does not have it or because the embedder did not grant
// it. It does not say whether one place can be used (a directory outside WASI's preopened ones
// fails when opened).

interface

type
  // The capabilities of the host, in the order of the fields of the HostSet.
  TCapability = (Console, Environment, Clock, Random, Memory, FileSystem, OS);

  // Whether a capability is there, or why not.
  TCapabilityStatus = (Available, Unsupported, NotPermitted);

  TCapabilities = record
  private
    class function GetConsole: Boolean; static;
    class function GetEnvironment: Boolean; static;
    class function GetClock: Boolean; static;
    class function GetRandom: Boolean; static;
    class function GetMemory: Boolean; static;
    class function GetFileSystem: Boolean; static;
    class function GetOS: Boolean; static;
  public
    class function Status(Capability: TCapability): TCapabilityStatus; static;
    class function Has(Capability: TCapability): Boolean; static;
    class property Console: Boolean read GetConsole;
    class property Environment: Boolean read GetEnvironment;
    class property Clock: Boolean read GetClock;
    class property Random: Boolean read GetRandom;
    class property Memory: Boolean read GetMemory;
    class property FileSystem: Boolean read GetFileSystem;
    class property OS: Boolean read GetOS;
  end;

implementation

uses
  // The Rust part of the unit (crates/puy-stdlib-ext; docs/design/stdlib.md 3.1)
  Puy.Platform.Native;

{ TCapabilities }

class function TCapabilities.Status(Capability: TCapability): TCapabilityStatus;
begin
  Result := TCapabilityStatus(CapabilityStatus(Ord(Capability)));
end;

class function TCapabilities.Has(Capability: TCapability): Boolean;
begin
  Result := Status(Capability) = TCapabilityStatus.Available;
end;

class function TCapabilities.GetConsole: Boolean;
begin
  Result := Has(TCapability.Console);
end;

class function TCapabilities.GetEnvironment: Boolean;
begin
  Result := Has(TCapability.Environment);
end;

class function TCapabilities.GetClock: Boolean;
begin
  Result := Has(TCapability.Clock);
end;

class function TCapabilities.GetRandom: Boolean;
begin
  Result := Has(TCapability.Random);
end;

class function TCapabilities.GetMemory: Boolean;
begin
  Result := Has(TCapability.Memory);
end;

class function TCapabilities.GetFileSystem: Boolean;
begin
  Result := Has(TCapability.FileSystem);
end;

class function TCapabilities.GetOS: Boolean;
begin
  Result := Has(TCapability.OS);
end;

end.
