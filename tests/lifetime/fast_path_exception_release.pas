// puy-test: memory-limit=1m
program FastPathExceptionRelease;

uses System.SysUtils;

// Exceptions raised by instructions on the VM's fast path (an array index out of range, a string
// index out of range, a division by zero) inside a routine with managed locals: the handler runs
// at the right place, and the locals are released when the routine ends. The loop would run out
// of the 1 MB limit if anything were kept (docs/design/vm.md 3.14, lifetime.md 3.8).

type
  TItem = record
    Name: string;
    Tags: array of string;
  end;

function Probe(Round, Zero: Integer): Integer;
var
  S: string;
  A: array of string;
  R: TItem = Default(TItem);
begin
  Result := 0;
  S := 'round' + IntToStr(Round);
  SetLength(A, 3);
  A[0] := S + '!';
  R.Name := A[0];
  R.Tags := A;
  try
    case Round mod 3 of
      0: S := A[Round];
      1: S := S + S[100];
      2: Result := Round div Zero;
    end;
  except
    on E: ERangeError do
      Result := 1;
    on E: EDivByZero do
      Result := 2;
  end;
  Result := Result * 10 + Length(R.Tags) + Length(S) - Length(R.Name);
end;

var
  Round, Total: Integer;
begin
  Total := 0;
  for Round := 3 to 5002 do
    Total := Total + Probe(Round, 0);
  WriteLn(Total);
end.
