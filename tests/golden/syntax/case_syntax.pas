program CaseSyntax;

// The syntax tree of `case` (semantics.md 4.10): CASE_STMT, CASE_ARM, CASE_LABEL (a value or a
// range) and CASE_ELSE.

var
  I: Integer;
begin
  I := 2;
  case I of
    1, 3..4: WriteLn('a');
    2: ;
  else
    WriteLn('b');
  end;
end.
