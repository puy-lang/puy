program MathFunctions;

// The functions of System (Sin, Cos, ArcTan, Ln, Exp, Int, Frac) and of System.Math: powers and
// logarithms, trigonometric and hyperbolic functions, conversions of angles. Printed with a
// fixed number of decimals, which does not depend on the last bits of the results. Arguments go
// through D, a Double: Delphi picks the Single overload of Tan, DegToRad, ... for an integer
// constant.

uses
  System.SysUtils, System.Math;

procedure Show(const Name: string; X: Double);
begin
  WriteLn(Name, ' ', X:0:9);
end;

// X as a Double.
function D(X: Double): Double;
begin
  Result := X;
end;

var
  S, C: Double;
begin
  Show('Sin', Sin(D(0.5)));
  Show('Cos', Cos(D(0.5)));
  Show('ArcTan', ArcTan(D(2)));
  Show('Ln', Ln(D(10)));
  Show('Exp', Exp(D(1.5)));
  Show('Int', Int(D(-2.75)));
  Show('Int', Int(D(3.5)));
  Show('Frac', Frac(D(-2.75)));
  Show('Frac', Frac(D(3.5)));
  Show('Sin of an Integer', Sin(1));

  Show('Power', Power(D(2), D(10)));
  Show('Power', Power(D(2.5), D(1.5)));
  Show('Power', Power(D(10), D(-2)));
  Show('IntPower', IntPower(D(3), 5));
  Show('IntPower', IntPower(D(2), -3));
  Show('IntPower', IntPower(D(1.5), 0));
  Show('Log10', Log10(D(1000)));
  Show('Log2', Log2(D(1024)));
  Show('LogN', LogN(D(3), D(81)));
  Show('LnXP1', LnXP1(D(0.5)));

  Show('Tan', Tan(D(1)));
  Show('Cotan', Cotan(D(1)));
  Show('Secant', Secant(D(1)));
  Show('Cosecant', Cosecant(D(1)));
  Show('ArcSin', ArcSin(D(0.5)));
  Show('ArcCos', ArcCos(D(0.5)));
  Show('ArcTan2', ArcTan2(D(1), D(-1)));
  Show('ArcTan2', ArcTan2(D(-2), D(3)));
  Show('Hypot', Hypot(D(3), D(4)));
  SinCos(D(1), S, C);
  Show('SinCos', S);
  Show('SinCos', C);

  Show('Sinh', Sinh(D(1)));
  Show('Cosh', Cosh(D(1)));
  Show('Tanh', Tanh(D(0.5)));
  Show('ArcSinh', ArcSinh(D(1)));
  Show('ArcCosh', ArcCosh(D(2)));
  Show('ArcTanh', ArcTanh(D(0.5)));

  Show('DegToRad', DegToRad(D(180)));
  Show('RadToDeg', RadToDeg(Pi / 3));
  Show('GradToRad', GradToRad(D(100)));
  Show('RadToGrad', RadToGrad(Pi));
  Show('CycleToRad', CycleToRad(D(0.25)));
  Show('RadToCycle', RadToCycle(Pi));
end.
