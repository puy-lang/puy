program ReturnValues;

// Values leaving a routine: a result made in the routine, a value parameter returned as is,
// `Exit(S)`, a substring of a parameter, and a call whose result is thrown away (lifetime.md 3.3, 3.5).

function Greet(const Name: string): string;
begin
  Result := 'Hello, ' + Name;
end;

function Same(S: string): string;
begin
  Result := S;
end;

function FirstWord(S: string): string;
var
  P: Integer;
begin
  P := Pos(' ', S);
  if P = 0 then
    Exit(S);
  Result := Copy(S, 1, P - 1);
end;

var
  A: string;
begin
  A := Greet('World');
  WriteLn(A);
  WriteLn(Same(A));
  WriteLn(FirstWord(A));
  WriteLn(FirstWord('single'));
  Greet('discarded');
end.
