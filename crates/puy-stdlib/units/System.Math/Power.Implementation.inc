// ===== Powers and logarithms =====
// The numbers of MathFunc1 / MathFunc2 are the constants of System.Math.pas.

function Power(const Base, Exponent: Double): Double;
begin
  Result := MathFunc2(Math2Power, Base, Exponent);
end;

function IntPower(const Base: Double; const Exponent: Integer): Double;
var
  B: Double;
  E: Int64;
begin
  // Int64, so that the magnitude of Low(Integer) fits
  E := Abs(Int64(Exponent));
  B := Base;
  Result := 1;
  while E > 0 do
  begin
    if Odd(E) then
      Result := Result * B;
    E := E div 2;
    if E > 0 then
      B := B * B;
  end;
  if Exponent < 0 then
    Result := 1 / Result;
end;

function Log10(const X: Double): Double;
begin
  Result := MathFunc1(MathLog10, X);
end;

function Log2(const X: Double): Double;
begin
  Result := MathFunc1(MathLog2, X);
end;

function LogN(const Base, X: Double): Double;
begin
  Result := Ln(X) / Ln(Base);
end;

function LnXP1(const X: Double): Double;
begin
  Result := MathFunc1(MathLnXP1, X);
end;
