program SliceFreed;

// The results of slices own their values and are freed (no leak; the runner checks the heap
// after the finishing stage): slices of strings, of arrays of strings and of a static array of
// strings, kept in a local, a global and an element, passed as an argument, and dropped as a
// temporary.

uses
  System.SysUtils;

type
  TNames = array[1..3] of string;

var
  G: TArray<string>;

function Join(const A: array of string): string;
var
  X: string;
begin
  Result := '';
  for X in A do
    Result := Result + X;
end;

procedure Run;
var
  S, T: string;
  A: TArray<string>;
  M: TArray<TArray<string>>;
  N: TNames = Default(TNames);
begin
  S := 'abc' + IntToStr(42);
  T := S[2..];
  A := ['a' + S, 'b' + S, 'c' + S];
  G := A[1..];
  SetLength(M, 1);
  M[0] := A[..1];
  N[1] := 'p' + S;
  N[2] := 'q' + S;
  N[3] := 'r' + S;
  WriteLn(T, ' ', Join(G), ' ', Join(M[0]), ' ', Join(N[2..]), ' ', Length(A[0..0][0]));
  A := A[1..1];
  WriteLn(Join(A));
end;

begin
  Run;
  WriteLn(Length(G));
end.
