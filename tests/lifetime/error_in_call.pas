program ErrorInCall;

// A runtime error in a nested call ends the program: the strings held by every frame on the way
// out are released (lifetime.md 3.8), so the leak check finds no block.

function Item(const S: string; I: Integer): Char;
var
  T: string;
begin
  T := S + '?';
  Result := T[I];
end;

procedure Outer(S: string);
var
  U: string;
begin
  U := S + S;
  WriteLn(Item(U, 100));
end;

begin
  Outer('ab');
end.
