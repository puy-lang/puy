// puy-test: memory-limit=64k
program AppendMemoryLimit;

// Growing a string or a dynamic array in place takes capacity to spare, but never past the
// isolate's memory limit: near the limit it grows by exactly what is needed, so the value can
// fill more than half of the limit, and a growth that does not fit raises EOutOfMemory and leaves
// the value as it was (types.md 9.1). Each value is grown in a routine of its own, so that what
// it holds is let go before the result is written.

uses System.SysUtils;

procedure FillString(var Kept, Filled: Boolean);
var
  S: string = '';
  N: Integer = 0;
begin
  try
    while True do
    begin
      S := S + '0123456789';
      Inc(N);
    end;
  except
    on E: EOutOfMemory do
    begin
      Kept := Length(S) = N * 10;
      Filled := Length(S) > 40000;
    end;
  end;
end;

procedure FillArray(var Kept, Filled: Boolean);
var
  A: array of Integer = nil;
  N: Integer = 0;
begin
  try
    while True do
    begin
      SetLength(A, Length(A) + 1);
      Inc(N);
    end;
  except
    on E: EOutOfMemory do
    begin
      Kept := Length(A) = N;
      Filled := Length(A) > 5000;
    end;
  end;
end;

var
  Kept: Boolean = False;
  Filled: Boolean = False;
begin
  FillString(Kept, Filled);
  WriteLn('string: ', Kept, ' ', Filled);
  FillArray(Kept, Filled);
  WriteLn('array: ', Kept, ' ', Filled);
end.
