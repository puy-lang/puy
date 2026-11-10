program CaseUnclosed;

// A `case` not closed at the end of the file: E0400 with `block starts here` at `case` and a fix
// that inserts the `end`s of the `case` and the `begin` (grammar.md chapter 3).

var
  I: Integer;
begin
  case I of
    1: WriteLn('a');
