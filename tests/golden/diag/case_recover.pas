program CaseRecover;

// Recovery from syntax errors in `case` statements (grammar.md chapter 3, semantics.md 4.10):
// a missing `of` before the first label, a missing `:` after a label, a missing `;` between arms,
// a `case` with no arm (E0403), a second statement in an arm, and a `case` closed by the
// `until` of an enclosing `repeat`. The statements after each error are read normally.

var
  I: Integer;
begin
  case I
    1: WriteLn('a');
  end;
  case I of
    1 WriteLn('a');
    2: WriteLn('b')
    3: WriteLn('c');
  end;
  case I of
  end;
  case I of
    1: WriteLn('a');
    2: WriteLn('b'); WriteLn('c');
  end;
  repeat
    case I of
      1: WriteLn('a');
  until I > 0;
  WriteLn('after');
end.
