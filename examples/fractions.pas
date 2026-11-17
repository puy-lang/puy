program Fractions;

uses
  System.SysUtils;

// Operator overloading on a record: arithmetic, comparison, negation,
// Inc / Dec, and the Implicit / Explicit conversions. A fraction is a value,
// so the operators return new records and never change their operands.

type
  EFractionError = class(Exception);

  TFraction = record
  private
    FNum, FDen: Int64;
    class function GCD(A, B: Int64): Int64; static;
  public
    constructor Create(ANum, ADen: Int64);
    function ToString: string;
    property Num: Int64 read FNum;
    property Den: Int64 read FDen;

    class operator Add(const A, B: TFraction): TFraction;
    class operator Subtract(const A, B: TFraction): TFraction;
    class operator Multiply(const A, B: TFraction): TFraction;
    class operator Divide(const A, B: TFraction): TFraction;
    class operator Negative(const A: TFraction): TFraction;
    class operator Equal(const A, B: TFraction): Boolean;
    // Equal fractions must have equal hash codes; both are in lowest terms.
    function GetHashCode: Int64;
    class operator NotEqual(const A, B: TFraction): Boolean;
    class operator LessThan(const A, B: TFraction): Boolean;
    class operator GreaterThan(const A, B: TFraction): Boolean;
    class operator Inc(const A: TFraction): TFraction;
    class operator Dec(const A: TFraction): TFraction;
    class operator Trunc(const A: TFraction): Int64;
    // An integer becomes a fraction wherever a TFraction is expected.
    class operator Implicit(N: Int64): TFraction;
    // A fraction becomes a Double only when asked for with Double(F).
    class operator Explicit(const A: TFraction): Double;
  end;

class function TFraction.GCD(A, B: Int64): Int64;
begin
  A := Abs(A);
  B := Abs(B);
  while B <> 0 do
  begin
    var T := A mod B;
    A := B;
    B := T;
  end;
  Result := A;
end;

// The constructor keeps every fraction in lowest terms with a positive
// denominator, so Equal can compare the fields.
constructor TFraction.Create(ANum, ADen: Int64);
begin
  if ADen = 0 then
    raise EFractionError.CreateFmt('%d/0: the denominator is zero', [ANum]);
  if ADen < 0 then
  begin
    ANum := -ANum;
    ADen := -ADen;
  end;
  var G := GCD(ANum, ADen);
  if G = 0 then
    G := 1;
  FNum := ANum div G;
  FDen := ADen div G;
end;

function TFraction.ToString: string;
begin
  if FDen = 1 then
    Result := IntToStr(FNum)
  else
    Result := IntToStr(FNum) + '/' + IntToStr(FDen);
end;

class operator TFraction.Add(const A, B: TFraction): TFraction;
begin
  Result := TFraction.Create(A.FNum * B.FDen + B.FNum * A.FDen, A.FDen * B.FDen);
end;

class operator TFraction.Subtract(const A, B: TFraction): TFraction;
begin
  Result := A + -B;
end;

class operator TFraction.Multiply(const A, B: TFraction): TFraction;
begin
  Result := TFraction.Create(A.FNum * B.FNum, A.FDen * B.FDen);
end;

class operator TFraction.Divide(const A, B: TFraction): TFraction;
begin
  Result := TFraction.Create(A.FNum * B.FDen, A.FDen * B.FNum);
end;

class operator TFraction.Negative(const A: TFraction): TFraction;
begin
  Result := TFraction.Create(-A.FNum, A.FDen);
end;

class operator TFraction.Equal(const A, B: TFraction): Boolean;
begin
  Result := (A.FNum = B.FNum) and (A.FDen = B.FDen);
end;

function TFraction.GetHashCode: Int64;
begin
  Result := HashCombine(FNum, FDen);
end;

class operator TFraction.NotEqual(const A, B: TFraction): Boolean;
begin
  Result := not (A = B);
end;

class operator TFraction.LessThan(const A, B: TFraction): Boolean;
begin
  Result := A.FNum * B.FDen < B.FNum * A.FDen;
end;

class operator TFraction.GreaterThan(const A, B: TFraction): Boolean;
begin
  Result := B < A;
end;

class operator TFraction.Inc(const A: TFraction): TFraction;
begin
  Result := A + 1;
end;

class operator TFraction.Dec(const A: TFraction): TFraction;
begin
  Result := A - 1;
end;

class operator TFraction.Trunc(const A: TFraction): Int64;
begin
  Result := A.FNum div A.FDen;
end;

class operator TFraction.Implicit(N: Int64): TFraction;
begin
  Result := TFraction.Create(N, 1);
end;

class operator TFraction.Explicit(const A: TFraction): Double;
begin
  Result := A.FNum / A.FDen;
end;

function F(Num, Den: Int64): TFraction;
begin
  Result := TFraction.Create(Num, Den);
end;

// The harmonic number 1 + 1/2 + ... + 1/N, added up exactly.
function Harmonic(N: Integer): TFraction;
begin
  Result := 0;
  for var I := 1 to N do
    Result := Result + F(1, I);
end;

var
  A, B, X: TFraction;

begin
  A := F(1, 2);
  B := F(3, 4);
  WriteLn('a = ', A.ToString, ', b = ', B.ToString);
  WriteLn('a + b = ', (A + B).ToString);
  WriteLn('a - b = ', (A - B).ToString);
  WriteLn('a * b = ', (A * B).ToString);
  WriteLn('a / b = ', (A / B).ToString);
  WriteLn('a + 2 = ', (A + 2).ToString);
  WriteLn('2 * a * b - 1 = ', (2 * A * B - 1).ToString);

  WriteLn('a < b: ', A < B, ', a > b: ', A > B);
  WriteLn('2/4 = 1/2: ', F(2, 4) = A, ', 6/3 = 2: ', F(6, 3) = 2);

  X := F(7, 3);
  WriteLn('x = ', X.ToString, ', Trunc(x) = ', Trunc(X), ', as a Double ',
    Double(X):0:4);
  Inc(X);
  WriteLn('after Inc: ', X.ToString);
  Dec(X);
  Dec(X);
  WriteLn('after Dec twice: ', X.ToString);

  for var N := 1 to 6 do
    WriteLn('H(', N, ') = ', Harmonic(N).ToString);

  try
    X := A / 0;
  except
    on E: EFractionError do
      WriteLn('error: ', E.Message);
  end;
end.
