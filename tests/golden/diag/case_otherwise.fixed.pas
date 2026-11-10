program CaseOtherwise;

// `otherwise` (FPC) is not the `else` of a `case` statement: E0400 with a fix that replaces it
// with `else` (semantics.md 4.10). A constant named `Otherwise` is still a label.

const
  Otherwise = 3;

var
  I: Integer;
begin
  I := 3;
  case I of
    Otherwise: WriteLn('three');
  else
    WriteLn('other');
  end;
end.
