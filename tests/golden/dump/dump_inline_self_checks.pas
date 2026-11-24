program DumpInlineSelfChecks;

{$Q-}
// The checks of `Self` before inlined methods (design/inlining.md 7.1): the second call on the
// same variable keeps no check, nor does a call on `Self` of a method; a call or `Free` in
// between keeps it; a loop without calls checks once before it (after the test that skips a loop
// of no iteration), and a loop with a call keeps the check in it.

type
  TBox = class
    Value: Integer;
    function Get: Integer;
    function Sum: Integer;
  end;

function TBox.Get: Integer;
begin
  Result := Value;
end;

function TBox.Sum: Integer;
begin
  Result := Get + Get;
end;

procedure Twice(B: TBox);
begin
  WriteLn(B.Get + B.Get);
end;

procedure AfterCall(B: TBox);
var
  X: Integer;
begin
  X := B.Get;
  WriteLn(X);
  WriteLn(X + B.Get);
end;

// Not run: freeing `B` and then calling a method on it raises.
procedure AfterFree(B: TBox);
var
  X: Integer;
begin
  X := B.Get;
  B.Free;
  WriteLn(X + B.Get);
end;

procedure Loops(B: TBox; N: Integer);
var
  I, S: Integer;
begin
  S := 0;
  for I := 1 to N do
    S := S + B.Get;
  for I := 1 to N do
    begin
      S := S + B.Get;
      WriteLn(S);
    end;
end;

var
  B: TBox;
begin
  B := TBox.Create;
  B.Value := 3;
  Twice(B);
  WriteLn(B.Sum);
  Loops(B, 2);
  AfterCall(B);
  // Never true: keeps `AfterFree` in the module (units.md 8.3)
  if ParamCount > 100 then
    AfterFree(B);
  B.Free;
end.
