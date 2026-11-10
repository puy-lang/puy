program RecoverMisc;

// No identifier after `.` in `uses` (E0405), `reference to` (E3200, skipped up to `;`),
// silently filling in a declaration identifier right after a lexical ERROR, `.` on a literal (E3200),
// a qualified name's `.` in the middle of skipping a statement (`with`, E3000), E0008 for `#$` (grammar.md 1.3 / chapter 3, lexical.md 3.5)

uses System.;

var
  P: reference to procedure(X: Integer);
  N: Int64;
  Café: Integer;
  あいう: Integer;
  S: string;
begin
  N := 1._5;
  N := 1.Foo;
  with System.Abs(N) do WriteLn;
  S := #$;
  WriteLn(N, S);
end.
