// puy-test: memory-limit=64k
program ReleaseWorkMemoryLimit;

// The work of a release job is counted against the isolate's memory limit (objects.md 14.8).
// Letting go of a large array whose first element is an instance with a destructor stops there
// to run it: the rest of the elements wait in the release job meanwhile, so the room left to the
// destructor is smaller than before the array was let go, although the array itself was freed.
// A large array without instances is let go first, so that the capacity the release's own work
// list keeps between calls is already counted before the room is measured.

uses System.SysUtils;

type
  TItem = class(TRefObject)
  public
    Measure: Boolean;
    destructor Destroy; override;
  end;

const
  N = 1000;

var
  During: Integer = 0;

// Whether a string of K characters can be made (ASCII text takes a byte a character); the
// string is let go on return
function Fits(K: Integer): Boolean;
var
  S: string;
begin
  try
    S := StringOfChar('x', K);
    Result := Length(S) = K;
  except
    on E: EOutOfMemory do
      Result := False;
  end;
end;

// Grows the release's work list to the capacity it keeps between calls
procedure WarmUp;
var
  W: array of string;
  S: string;
  I: Integer;
begin
  S := StringOfChar('x', 1);
  SetLength(W, 2 * N);
  for I := 0 to 2 * N - 1 do
    W[I] := S;
  {$WARN unused-value OFF}
  W := nil;
  {$WARN unused-value ON}
end;

// The longest string that can still be made
function Room: Integer;
var
  Lo, Hi, Mid: Integer;
begin
  Lo := 0;
  Hi := 65536;
  while Lo < Hi do
  begin
    Mid := (Lo + Hi + 1) div 2;
    if Fits(Mid) then
      Lo := Mid
    else
      Hi := Mid - 1;
  end;
  Result := Lo;
end;

destructor TItem.Destroy;
begin
  if Measure then
    During := Room;
end;

procedure Run;
var
  A: array of TItem;
  First, Shared: TItem;
  I, Before: Integer;
begin
  SetLength(A, N);
  First := TItem.Create;
  First.Measure := True;
  A[0] := First;
  First := nil;
  Shared := TItem.Create;
  for I := 1 to N - 1 do
    A[I] := Shared;
  Shared := nil;
  WarmUp;
  Before := Room;
  {$WARN unused-value OFF}
  A := nil;
  {$WARN unused-value ON}
  // The array (8 bytes an element) was freed, and its remaining N - 1 elements (16 bytes each)
  // were waiting in the release job while the destructor ran
  WriteLn('counted: ', During < Before - 4 * N);
end;

begin
  Run;
end.
