program CaseStmt;

// `case` (semantics.md 4.10): integer, `Char`, `Boolean`, enumeration and subrange selectors,
// ranges and lists of labels, with and without `else`, and a selector with a side effect that is
// evaluated only once. A `string` selector is in golden/runtime/case_string (FPC 3.2 does not
// compile a `case` over a `UnicodeString`).

type
  TColor = (Red, Green, Blue, Yellow);
  TDigit = 0..9;

var
  I: Integer;
  B: Byte;
  K: Int64;
  C: Char;
  Flag: Boolean;
  Col: TColor;
  D: TDigit;
  Calls: Integer;

function Next: Integer;
begin
  Inc(Calls);
  Result := Calls * 2;
end;

function Kind(N: Integer): string;
begin
  case N of
    -5..-1: Result := 'negative';
    0: Result := 'zero';
    1, 3, 5, 7, 9: Result := 'odd digit';
    2, 4, 6, 8: Result := 'even digit';
    10..99: Result := 'two digits';
  else
    Result := 'large';
  end;
end;

begin
  for I := -3 to 12 do
    Write(Kind(I), ' ');
  WriteLn;
  WriteLn(Kind(100), ' ', Kind(-6));

  // Without `else`: nothing happens when no label matches
  B := 200;
  case B of
    0..99: WriteLn('small');
    100..199: WriteLn('medium');
  end;
  WriteLn('after byte');

  K := 5000000000;
  case K of
    0..4999999999: WriteLn('below');
    5000000000: WriteLn('exactly');
  else
    WriteLn('above');
  end;

  for C in 'a5Z_ ' do
    case C of
      'a'..'z': WriteLn(C, ' lower');
      'A'..'Z': WriteLn(C, ' upper');
      '0'..'9': WriteLn(C, ' digit');
      ' ', #9: WriteLn('blank');
    else
      WriteLn(C, ' other');
    end;

  Flag := False;
  case Flag of
    True: WriteLn('yes');
    False: WriteLn('no');
  end;

  for Col := Low(TColor) to High(TColor) do
    case Col of
      TColor.Red: WriteLn('red');
      TColor.Green..TColor.Blue: WriteLn('green or blue');
    else
      WriteLn('another color');
    end;

  D := 7;
  case D of
    0..4: WriteLn('low digit');
    5..9: WriteLn('high digit');
  end;

  // The selector is evaluated once, before the labels are compared
  Calls := 0;
  case Next of
    1: WriteLn('one');
    2: WriteLn('two');
    4: WriteLn('four');
  end;
  WriteLn('calls: ', Calls);

  // A compound statement in an arm, and nested `case` statements
  for I := 1 to 3 do
    case I of
      1:
        begin
          Write('a');
          WriteLn('b');
        end;
      2, 3:
        case I * 10 of
          20: WriteLn('twenty');
        else
          WriteLn('other: ', I * 10);
        end;
    end;
end.
