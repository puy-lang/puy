program RealFormat;

// The default text of reals and Currency (semantics.md 6.1): the shortest digits that read
// back to the same value, fixed notation for exponents -5..14, `INF` / `-INF` / `NAN` / `-0`,
// a Single as binary32, `X:W`, `X:W:D` rounding the exact binary value ties to even, the
// constant `1 / 0`, and the round trip of `FloatToStr` / `StrToFloat` (5.3).

uses System.SysUtils;

const
  Inf = 1 / 0;
  Third = 1 / 3;

var
  D, Z: Double;
  S: Single;
  C: Currency;
  T: string;

begin
  D := 0.1;
  Z := 0;
  WriteLn(D + 0.2);
  WriteLn(1E20, ' ', 1.5E-7, ' ', 1E15, ' ', 123456789012345.0, ' ', 1E-5, ' ', 0.0001);
  WriteLn(100.0, ' ', 1.5, ' ', -2.5E-10, ' ', 5E-324, ' ', 1.7976931348623157E308);
  WriteLn(Inf, ' ', -Inf, ' ', Z / Z, ' ', -Z, ' ', 1 / Z, ' ', -1 / Z);
  WriteLn(Third, ' ', Pi, ' ', Sqrt(-1));
  S := 0.1;
  WriteLn(S, ' ', S * 3, ' ', S + D, ' ', Single(16777217), ' ', Single(1E40));
  C := 1.5;
  WriteLn(C, ' ', C * 66.6667, ' ', Currency(0.0002), ' ', -C, ' ', C - C, ' ', C / 3);
  WriteLn(Currency(922337203685477) + Currency(0.5807), ' ', Currency(0.00005), ' ', Currency(0.03125));
  WriteLn(D:8, '|', C:6, '|', Inf:5, '|', D:2, '|');
  WriteLn(0.125:0:2, ' ', 0.375:0:2, ' ', 2.5:0:0, ' ', 3.5:0:0, ' ', 2.675:0:2, ' ', D:0:20);
  WriteLn(1.5:0:-1, ' ', Inf:0:2, ' ', -Inf:8:2, ' ', (Z / Z):0:2, ' ', -0.001:0:2);
  WriteLn(C:0:0, ' ', Currency(2.5):0:0, ' ', Currency(0.125):0:2, ' ', C:0:6, ' ', -C:7:1);
  T := FloatToStr(D + 0.2);
  WriteLn(T, ' ', StrToFloat(T) = D + 0.2);
  WriteLn(FloatToStr(S), ' ', FloatToStr(C), ' ', FloatToStr(7), ' ', FloatToStr(Inf));
  WriteLn(StrToFloat('  -1.5e3'), ' ', StrToFloat('.5'), ' ', StrToFloat('-INF'), ' ', StrToFloat('nan'));
end.
