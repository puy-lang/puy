program DumpCase;

// The `hir` and `bytecode` dumps of `case` (semantics.md 9, bytecode.md 5): the selector, the
// labels of each arm (a range has two constants) and the `else` part; the selector is evaluated
// once into a register and compared with each label in order.

var
  I: Integer;
  S: string;
begin
  I := 4;
  case I + 1 of
    1, 3..5: WriteLn('a');
    2: ;
  else
    WriteLn('b');
  end;
  S := 'x';
  case S of
    'x': WriteLn('x');
  end;
end.
