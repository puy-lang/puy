program OperatorOrdinalErrors;

// A type with operators is still not a constant or an ordinal type (objects.md 11.3, 11.4): an
// operator in a constant expression is E0713, and a `case` selector (E0739), a `Write` argument
// (E0718) and a `for..to` loop variable (E0722) keep the errors of M2 even with `Implicit`.

type
  TV = record
    X: Integer;
    class operator Add(const A, B: TV): TV;
    class operator Implicit(N: Integer): TV;
    class operator Implicit(const A: TV): Integer;
    class operator Implicit(const A: TV): string;
  end;

class operator TV.Add(const A, B: TV): TV;
begin
  Result := Default(TV);
  Result.X := A.X + B.X;
end;

class operator TV.Implicit(N: Integer): TV;
begin
  Result := Default(TV);
  Result.X := N;
end;

class operator TV.Implicit(const A: TV): Integer;
begin
  Result := A.X;
end;

class operator TV.Implicit(const A: TV): string;
begin
  Result := 'v';
end;

const
  Zero: TV = (X: 0);
  Two = Zero + Zero;                               // E0713

var
  A: TV;
begin
  A := 1;
  case A of                                        // E0739
    1: WriteLn('one');
  end;
  WriteLn(A);                                      // E0718
  for A := 1 to 3 do                               // E0722
    WriteLn('x');
end.
