program FormatReals;

// Format results Puy defines differently from FPC (semantics.md 5.3, 6.1): `e` and `g` without a
// precision use the shortest digits that read back, rounding is of the exact binary value with
// ties to even, a negative value rounded to 0 keeps its sign, `g` with a precision picks fixed
// or exponent notation by the rule of 6.1, `u` / `x` and IntToHex of a narrow type use that
// type's width, and an out of range TryStrToInt fails.

uses System.SysUtils;

var
  D: Double;
  F: Single;
  C: Currency;
  S: ShortInt;
  M: SmallInt;
  V: Integer;
begin
  D := 1234.5;
  F := 0.1;
  C := 12.5;
  // Defaults: `e` shortest, `g` like FloatToStr, `f` / `n` two decimals
  WriteLn(Format('[%e] [%g] [%f] [%n]', [D, D, D, D]));
  WriteLn(Format('[%e] [%g] [%e] [%g]', [F, F, 1E20, 1E-7]));
  WriteLn(Format('[%e] [%g] [%f] [%n]', [C, C, C, C]));
  // Ties to even on the exact value, and the sign of a negative value rounded to 0
  WriteLn(Format('[%.0f] [%.0f] [%.2f] [%.2f] [%.4g]', [2.5, 3.5, 1.005, -0.001, D]));
  // `g` with a precision: fixed unless the exponent is outside -5..14
  WriteLn(Format('[%.2g] [%.3g] [%.17g] [%.1e] [%.0e]', [D, 1E15, 0.1, 0.000123, 5.0]));
  // INF and NAN
  D := 0.0;
  WriteLn(Format('[%f] [%e] [%g] [%n]', [1 / D, -1 / D, D / D, 1 / D]));
  // A narrow integer type's width for `u`, `x` and IntToHex
  S := -1;
  M := -2;
  WriteLn(Format('[%u] [%x] [%u] [%x]', [S, S, M, M]));
  WriteLn(IntToHex(S, 1), ' ', IntToHex(M, 1));
  // Out of the range of `Integer`
  WriteLn(TryStrToInt('2147483648', V), ' ', V);
  WriteLn(TryStrToInt('-2147483648', V), ' ', V);
end.
