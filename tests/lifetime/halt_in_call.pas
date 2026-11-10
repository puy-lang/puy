program HaltInCall;

// `Halt` in a nested call ends the program: the strings held by every frame are released
// (lifetime.md 3.8), so the leak check finds no block.

procedure Inner(S: string);
var
  T: string;
begin
  T := S + '!';
  WriteLn(T);
  Halt(3);
end;

procedure Outer(const S: string);
var
  U: string;
begin
  U := S + S;
  Inner(U);
end;

begin
  Outer('ab');
end.
