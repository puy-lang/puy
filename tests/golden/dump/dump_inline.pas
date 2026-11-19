program DumpInline;

{$Q-}
// The `bytecode` dump of inlined calls (design/inlining.md, bytecode.md 5): `.inline` and
// `.endinline` around the code standing for a callee's body, nested for a call inlined into an
// inlined routine; a method's inlined call checks `Self` first (`ClassType`); the arguments
// moved from variables are read from the variables; a recursive routine, a routine with a
// handler table and a routine releasing a managed value at its end keep their calls.

uses System.SysUtils;

type
  TCounter = class
    Count: Integer;
    function Next: Integer;
  end;

function TCounter.Next: Integer;
begin
  Count := Count + 1;
  Result := Count;
end;

{$Q+}
function Add(A, B: Integer): Integer;
begin
  Result := A + B;
end;
{$Q-}

function AddTwice(A, B: Integer): Integer;
begin
  Result := Add(Add(A, B), B);
end;

function Fact(N: Integer): Integer;
begin
  if N <= 1 then
    Result := 1
  else
    Result := N * Fact(N - 1);
end;

function Safe(N: Integer): Integer;
begin
  try
    Result := 100 div N;
  except
    Result := 0;
  end;
end;

function Name(N: Integer): string;
begin
  Result := 'n' + IntToStr(N);
end;

var
  C: TCounter;
  X, Y: Integer;
begin
  C := TCounter.Create;
  X := 2;
  Y := 3;
  WriteLn(AddTwice(X, Y), ' ', C.Next, ' ', Fact(4), ' ', Safe(0), ' ', Name(X));
  C.Free;
end.
