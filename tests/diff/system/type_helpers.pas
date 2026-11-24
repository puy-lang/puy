program TypeHelpersCase;

// The members of the helpers of the integer types, Double and Boolean
// (spec/stdlib/System/Helpers.md) in the cases where Puy, Delphi and FPC agree: the constants
// of the range, Parse and TryParse, the hexadecimal digits asked for, the conversions, and the
// tests of a Double. Char's helper (FPC has none), ToHexString without digits (Delphi XE4 writes
// the fewest digits, FPC the width of the type) and with digits on the types other than Integer
// and Int64 (XE4 does not write the value), the value TryParse leaves on a failure (XE4
// keeps what it read), the bytes and the bits, the special values of Double, the Unicode cases
// and the errors are in golden/runtime/type_helpers_puy.

uses
  System.SysUtils;

var
  I: Integer;
  L: Int64;
  D: Double;
  X: Boolean;
begin
  // The range of each type
  WriteLn(Integer.MaxValue, ' ', Integer.MinValue);
  WriteLn(Byte.MaxValue, ' ', ShortInt.MinValue, ' ', Word.MaxValue, ' ', SmallInt.MinValue);
  WriteLn(Cardinal.MaxValue, ' ', Int64.MinValue);

  // Parse and TryParse
  WriteLn(Integer.Parse('12') + 1, ' ', Integer.Parse('-7'), ' ', Integer.Parse('$1F'));
  WriteLn(Int64.Parse('-123'), ' ', Byte.Parse('200'));
  WriteLn(Integer.TryParse('42', I), ' ', I);
  WriteLn(Integer.TryParse('', I), ' ', I);

  // Hexadecimal digits: at least the digits asked for
  I := 255;
  L := -2;
  WriteLn(I.ToHexString(1), ' ', I.ToHexString(6));
  WriteLn(L.ToHexString(16));

  // Conversions
  WriteLn(I.ToBoolean, ' ', Integer(0).ToBoolean);
  WriteLn(I.ToDouble:0:1, ' ', L.ToDouble:0:1);

  // Double
  D := 1.5;
  WriteLn(D.IsNan, ' ', D.IsInfinity, ' ', D.IsNegativeInfinity, ' ', D.IsPositiveInfinity);
  WriteLn(Double.Parse('2.5'):0:2, ' ', Double.Parse('-1e3'):0:1);
  WriteLn(Double.TryParse('0.25', D), ' ', D:0:2);
  WriteLn(Double.TryParse('x', D));

  // Boolean
  WriteLn(Boolean.Parse('true'), ' ', Boolean.Parse('FALSE'), ' ', Boolean.Parse('5'));
  X := True;
  WriteLn(X.ToInteger, ' ', False.ToInteger);
end.
