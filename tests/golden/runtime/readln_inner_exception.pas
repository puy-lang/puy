program ReadLnInnerException;

// An exception raised and caught inside a routine called from the indices of a `ReadLn` does
// not end that `ReadLn` (semantics.md 6.2): it still reads the rest of its line. A `Read` in such
// a routine reads across lines as usual and does not end the outer `ReadLn` either.

uses System.SysUtils;

var
  A, K: Integer;
  B: array[0..1] of Integer;

function Idx: Integer;
begin
  try
    raise Exception.Create('x');
  except
  end;
  Result := 0;
end;

function ReadIdx: Integer;
begin
  Read(K);
  Result := 1;
end;

begin
  ReadLn(A, B[Idx]);
  WriteLn(A, ' ', B[0]);
  ReadLn(A, B[ReadIdx]);
  WriteLn(A, ' ', K, ' ', B[1]);
  ReadLn(A);
  WriteLn(A);
end.
