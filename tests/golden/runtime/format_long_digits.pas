program FormatLongDigits;

// Precisions past the 65535 digits Rust's formatting takes, and the `,` groups of `n`
// (semantics.md 5.3, 6.1): the digits past the exact value of a real are zeros, written after
// it (`f`) or before the exponent (`e`), and `n` groups only the integer part.

uses System.SysUtils;

var
  S: string;
  C: Currency;
begin
  S := Format('%.70000f', [0.1]);
  WriteLn(Length(S), ' ', Copy(S, 1, 60), ' ', Copy(S, Length(S) - 3, 4));
  S := Format('%.70000e', [-1.5]);
  WriteLn(Length(S), ' ', Copy(S, 1, 6), ' ', Copy(S, Length(S) - 7, 8));
  S := Format('%.70000d', [-42]);
  WriteLn(Length(S), ' ', Copy(S, 1, 4), ' ', Copy(S, Length(S) - 3, 4));
  S := Format('%.70000n', [1234567.25]);
  WriteLn(Length(S), ' ', Copy(S, 1, 16), ' ', Copy(S, Length(S) - 3, 4));
  C := -9876543.21;
  WriteLn(Format('[%n] [%.0n] [%.6n]', [C, C, C]));
  WriteLn(Format('[%n] [%n] [%n] [%n]', [1.0, 12.0, 123.0, 1234.0]));
  WriteLn(Format('[%n] [%n] [%.0n]', [-123456.0, 1E15, 999.5]));
end.
