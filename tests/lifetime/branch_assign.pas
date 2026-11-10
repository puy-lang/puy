program BranchAssign;

uses System.SysUtils;

// Assigning a string on both sides of a branch: each side makes the value straight into the
// variable, which lets go of its old value (lifetime.md 3.1). Concatenating with the empty string
// shares the other operand instead of copying it.

procedure Show(N: Integer);
var
  S, T: string;
begin
  if N > 0 then
    S := 'positive'
  else
    S := IntToStr(N);
  T := '';
  T := T + S;
  T := S + '';
  WriteLn(T);
end;

begin
  Show(1);
  Show(-2);
end.
