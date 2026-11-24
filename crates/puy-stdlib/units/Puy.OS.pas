unit Puy.OS;

// The OS functions Delphi's units do not have (docs/spec/stdlib/Puy.OS/README.md): the process
// ID, the number of processors, and writing, removing and listing environment variables. The
// values come from the host through Puy.OS.Native (crates/puy-stdlib-ext). A variable written
// here is the isolate's own: GetEnvironmentVariable (System.SysUtils) in the same isolate sees
// it, the process's variables and other isolates do not (docs/design/stdlib.md 3.4.3).

interface

type
  // One environment variable of GetEnvironmentVariables.
  TEnvironmentVariable = record
    Name: string;
    Value: string;
  end;

// The ID of the process; 0 where there is none (WASI, the browser) or without the OS information.
function GetProcessId: Cardinal;

// The number of logical processors the process may use; at least 1.
function GetProcessorCount: Integer;

// Sets the environment variable Name to Value for this isolate. EArgumentException if Name is
// empty or contains '=' or #0, or Value contains #0.
procedure SetEnvironmentVariable(const Name, Value: string);

// Removes the environment variable Name for this isolate (nothing happens if it is not set).
// EArgumentException for a Name SetEnvironmentVariable refuses.
procedure RemoveEnvironmentVariable(const Name: string);

// Every environment variable as this isolate sees it: the host's in its order (with the
// isolate's changes), then the names only this isolate set, in the order they were first set.
function GetEnvironmentVariables: TArray<TEnvironmentVariable>;

implementation

uses
  // The Rust part of the unit (crates/puy-stdlib-ext; docs/design/stdlib.md 3.1)
  Puy.OS.Native;

function GetProcessId: Cardinal;
begin
  Result := ProcessId;
end;

function GetProcessorCount: Integer;
begin
  Result := ProcessorCount;
end;

// The name and the value are checked by EnvTable.set in crates/puy-ext/src/env.rs (through
// Puy.OS.Native's SetEnv / RemoveEnv): EArgumentException for a name GetEnvironmentVariable
// answers '' for without asking the host, or a value with #0.
procedure SetEnvironmentVariable(const Name, Value: string);
begin
  SetEnv(Name, Value);
end;

procedure RemoveEnvironmentVariable(const Name: string);
begin
  RemoveEnv(Name);
end;

function GetEnvironmentVariables: TArray<TEnvironmentVariable>;
begin
  var Vars := EnvVars;
  SetLength(Result, Length(Vars));
  for var I := 0 to High(Vars) do
  begin
    Result[I].Name := Vars[I].Name;
    Result[I].Value := Vars[I].Value;
  end;
end;

end.
