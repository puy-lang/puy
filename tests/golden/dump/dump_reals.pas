program DumpReals;

// The `hir` and `bytecode` dumps of reals and Currency: `float` constants (a Single constant
// is an `f64` rounded when loaded), the `num->` conversions, `real:` and `curr` operations,
// comparisons that are not fused into a branch, `X:W:D`, and the real built-ins.

uses System.SysUtils;

type
  TPoint = record
    X: Single;
    Y: Double;
  end;

const
  Origin: TPoint = (X: 0.5; Y: -1.25);

var
  D: Double;
  S: Single;
  C: Currency;
  I: Integer;
  P: TPoint;

begin
  I := 3;
  S := 0.1;
  D := I / 2 + S;
  C := 1.5;
  C := C * I - 0.25;
  P := Origin;
  if D >= P.Y then
    WriteLn(D:8:2, ' ', -C, ' ', Trunc(D), ' ', Round(C), ' ', P.X);
  WriteLn(Sqrt(D), ' ', Abs(-S), ' ', Sqr(C), ' ', FloatToStr(S), ' ', StrToFloat('2.5'));
  WriteLn(D <> D, ' ', C = I, ' ', 1 / 0, ' ', Pi);
end.
