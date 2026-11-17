// puy-test: memory-limit=64k
program ConcatMemoryLimit;

// The intermediate value of `(X + Y) + Z` is extended in place when nothing else refers to it
// (bytecode.md ConcatStr; without the parentheses the sum is made at once by ConcatStrN). Near
// the isolate's memory limit the extension raises EOutOfMemory like making a new string would,
// and leaves the operands as they were. The intermediate value is made apart from `T`, so `T`
// keeps the value of the previous iteration (semantics.md 3.1).

uses System.SysUtils;

procedure Fill(var Kept, Reached: Boolean);
var
  S: string = '';
  Big: string = '';
  T: string = '';
  I, N: Integer;
  Stage: Integer = 0;
begin
  for I := 1 to 200 do
    Big := Big + '0123456789';
  N := 0;
  try
    while True do
    begin
      Stage := 0;
      S := S + '0123456789';
      Stage := 1;
      T := ('<' + S) + Big;
      Inc(N);
    end;
  except
    on E: EOutOfMemory do
    begin
      // Stage 1: the concatenation failed after S was extended
      Kept := (Length(Big) = 2000) and (Copy(Big, 1991, 10) = '0123456789')
        and (Length(S) = (N + 1) * 10) and (Copy(S, Length(S) - 9, 10) = '0123456789')
        and (Length(T) = 1 + N * 10 + 2000) and (Copy(T, 1, 1) = '<')
        and (Copy(T, N * 10 - 8, 10) = '0123456789')
        and (Copy(T, Length(T) - 9, 10) = '0123456789');
      Reached := (Stage = 1) and (N > 100);
    end;
  end;
end;

var
  Kept: Boolean = False;
  Reached: Boolean = False;
begin
  Fill(Kept, Reached);
  WriteLn('concat: ', Kept, ' ', Reached);
end.
