program HelperSubrange;

// A subrange type's own helper is found through a value of that type (objects.md 10.2, 10.4
// step 3): a variable (also in parentheses), a function's result and an enumeration subrange,
// before the base type's helpers, which still apply (`D.Twice`, the built-in `D.ToString`).

type
  TColor = (Red, Green, Blue);
  TDigit = 0..9;
  TWarm = TColor.Green..TColor.Blue;

  TDigitHelper = record helper for TDigit
    function Neg: Integer;
  end;

  TWarmHelper = record helper for TWarm
    function Name: string;
  end;

  TIntHelper = record helper for Integer
    function Twice: Integer;
  end;

function TDigitHelper.Neg: Integer;
begin
  Result := -Self;
end;

function TWarmHelper.Name: string;
begin
  if Self = TColor.Green then
    Result := 'green'
  else
    Result := 'blue';
end;

function TIntHelper.Twice: Integer;
begin
  Result := Self * 2;
end;

function Seven: TDigit;
begin
  Result := 7;
end;

var
  D: TDigit;
  W: TWarm;
  N: Integer;
begin
  D := 3;
  W := TColor.Blue;
  N := 5;
  WriteLn(D.Neg, ' ', D.Neg(), ' ', (D).Neg, ' ', Seven.Neg, ' ', W.Name);
  WriteLn(D.Twice, ' ', N.Twice, ' ', D.ToString);
end.
