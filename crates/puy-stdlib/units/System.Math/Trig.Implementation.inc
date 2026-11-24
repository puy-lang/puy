// ===== Angles, trigonometric and hyperbolic functions =====
// The numbers of MathFunc1 / MathFunc2 are the constants of System.Math.pas.

function Tan(const X: Double): Double;
begin
  Result := MathFunc1(MathTan, X);
end;

function Cotan(const X: Double): Double;
begin
  Result := 1 / Tan(X);
end;

function Cot(const X: Double): Double;
begin
  Result := 1 / Tan(X);
end;

function Secant(const X: Double): Double;
begin
  Result := 1 / Cos(X);
end;

function Sec(const X: Double): Double;
begin
  Result := 1 / Cos(X);
end;

function Cosecant(const X: Double): Double;
begin
  Result := 1 / Sin(X);
end;

function Csc(const X: Double): Double;
begin
  Result := 1 / Sin(X);
end;

procedure SinCos(const Theta: Double; var Sin, Cos: Double);
begin
  Sin := System.Sin(Theta);
  Cos := System.Cos(Theta);
end;

function ArcSin(const X: Double): Double;
begin
  Result := MathFunc1(MathArcSin, X);
end;

function ArcCos(const X: Double): Double;
begin
  Result := MathFunc1(MathArcCos, X);
end;

function ArcTan2(const Y, X: Double): Double;
begin
  Result := MathFunc2(Math2ArcTan2, Y, X);
end;

function ArcCot(const X: Double): Double;
begin
  Result := ArcTan(1 / X);
end;

function ArcSec(const X: Double): Double;
begin
  Result := ArcCos(1 / X);
end;

function ArcCsc(const X: Double): Double;
begin
  Result := ArcSin(1 / X);
end;

function Hypot(const X, Y: Double): Double;
begin
  Result := MathFunc2(Math2Hypot, X, Y);
end;

function Sinh(const X: Double): Double;
begin
  Result := MathFunc1(MathSinh, X);
end;

function Cosh(const X: Double): Double;
begin
  Result := MathFunc1(MathCosh, X);
end;

function Tanh(const X: Double): Double;
begin
  Result := MathFunc1(MathTanh, X);
end;

function CotH(const X: Double): Double;
begin
  Result := 1 / Tanh(X);
end;

function SecH(const X: Double): Double;
begin
  Result := 1 / Cosh(X);
end;

function CscH(const X: Double): Double;
begin
  Result := 1 / Sinh(X);
end;

function ArcSinh(const X: Double): Double;
begin
  Result := MathFunc1(MathArcSinh, X);
end;

function ArcCosh(const X: Double): Double;
begin
  Result := MathFunc1(MathArcCosh, X);
end;

function ArcTanh(const X: Double): Double;
begin
  Result := MathFunc1(MathArcTanh, X);
end;

function ArcCotH(const X: Double): Double;
begin
  Result := ArcTanh(1 / X);
end;

function ArcSecH(const X: Double): Double;
begin
  Result := ArcCosh(1 / X);
end;

function ArcCscH(const X: Double): Double;
begin
  Result := ArcSinh(1 / X);
end;

function DegToRad(const Degrees: Double): Double;
begin
  Result := Degrees * (Pi / 180);
end;

function RadToDeg(const Radians: Double): Double;
begin
  Result := Radians * (180 / Pi);
end;

function GradToRad(const Grads: Double): Double;
begin
  Result := Grads * (Pi / 200);
end;

function RadToGrad(const Radians: Double): Double;
begin
  Result := Radians * (200 / Pi);
end;

function CycleToRad(const Cycles: Double): Double;
begin
  Result := Cycles * (2 * Pi);
end;

function RadToCycle(const Radians: Double): Double;
begin
  Result := Radians / (2 * Pi);
end;
