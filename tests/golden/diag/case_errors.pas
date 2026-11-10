program CaseErrors;

// Errors in `case` statements (semantics.md 4.10): the three forms of E0709 (a duplicate label
// with the previous label as `secondary`, an empty range, a range in a `string` case), E0739 for
// a selector that is neither ordinal nor `string`, and the checks of a label: E0702 (not
// assignment compatible), E0713 (not a constant), E0714 (outside the selector's type, including a
// subrange) and E0736 (an unqualified enumeration value).

type
  TColor = (Red, Green, Blue);
  TDigit = 0..9;
  TPoint = record
    X: Integer;
  end;

var
  I: Integer;
  B: Byte;
  D: TDigit;
  S: string;
  P: TPoint;
  A: TArray<Integer>;
  Col: TColor;
begin
  case I of
    1: WriteLn('a');
    2, 1: WriteLn('b');
    3..6: WriteLn('c');
    5: WriteLn('d');
    0..10: WriteLn('e');
    9..7: WriteLn('f');
  end;
  case Col of
    TColor.Red..TColor.Green: WriteLn('a');
    TColor.Green: WriteLn('b');
  end;
  case S of
    'a'..'c': WriteLn('a');
    'x': WriteLn('b');
    'x', 'X': WriteLn('c');
    'y': WriteLn('d');
    'y': WriteLn('e');
  end;
  case P of
    1: WriteLn('a');
  end;
  case A of
    1: WriteLn('a');
  end;
  case I of
    'x': WriteLn('a');
    I: WriteLn('b');
    TColor.Red: WriteLn('c');
  end;
  case B of
    255: WriteLn('a');
    256: WriteLn('b');
  end;
  case D of
    9: WriteLn('a');
    10: WriteLn('b');
  end;
  case Col of
    Red: WriteLn('a');
  end;
end.
