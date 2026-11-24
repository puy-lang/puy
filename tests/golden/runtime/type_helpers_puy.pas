program TypeHelpersPuy;

// The helpers of the integer types, Single, Double, Boolean and Char
// (spec/stdlib/System/Helpers.md) where Puy decides the result: the members reached through the
// name of the type (`Integer.Parse`, `Char.IsDigit(C)`, `string.Join`), Parse beyond Integer and out of the
// type's range, TryParse leaving 0, ToHexString with the width of the type (Delphi XE4 writes
// the fewest digits), the bytes (little-endian) and the bits of a value, the
// special values of Single and Double, Char's Unicode properties and case mapping, and the
// uncaught ERangeError of a Parse out of range. The cases Delphi and FPC agree with are in
// diff/system/type_helpers.

procedure ShowBytes(const Bytes: TArray<Byte>);
begin
  Write(Length(Bytes), ':');
  for var B in Bytes do
    Write(' ', B);
  WriteLn;
end;

procedure Fails(const What: string; const P: TProc);
begin
  try
    P();
    WriteLn(What, ': no exception');
  except
    on E: Exception do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

type
  TDigit = 0..9;
  TCount = type Integer;
  TText = string;

begin
  // Parse: StrToInt64's form, then the range of the type
  WriteLn(Int64.Parse('9000000000'), ' ', Int64.Parse('$7FFFFFFFFFFFFFFF'));
  WriteLn(UInt64.Parse('18446744073709551615'), ' ', UInt64.Parse(' +$FFFFFFFFFFFFFFFF'));
  WriteLn(Cardinal.Parse('4294967295'), ' ', ShortInt.Parse('-128'));
  Fails('Byte.Parse(256)', procedure begin Byte.Parse('256') end);
  Fails('Integer.Parse(3000000000)', procedure begin Integer.Parse('3000000000') end);
  Fails('Integer.Parse(1.5)', procedure begin Integer.Parse('1.5') end);
  Fails('UInt64.Parse(-1)', procedure begin UInt64.Parse('-1') end);
  Fails('UInt64.Parse(2^64)', procedure begin UInt64.Parse('18446744073709551616') end);
  Fails('Double.Parse(abc)', procedure begin Double.Parse('abc') end);
  Fails('Boolean.Parse(yes)', procedure begin Boolean.Parse('yes') end);

  // TryParse: False and 0 when it does not read, also after digits
  var N: Integer := 9;
  WriteLn(Integer.TryParse('4x', N), ' ', N);
  var B: Byte := 9;
  var U: UInt64 := 9;
  var S: Single := 9;
  var Flag := True;
  WriteLn(Byte.TryParse('256', B), ' ', B, ' ', Byte.TryParse('255', B), ' ', B);
  WriteLn(UInt64.TryParse('18446744073709551616', U), ' ', U);
  WriteLn(UInt64.TryParse('$10', U), ' ', U, ' ', UInt64.TryParse('-0', U), ' ', U);
  WriteLn(Single.TryParse('x', S), ' ', S, ' ', Single.TryParse('0.5', S), ' ', S);
  WriteLn(Boolean.TryParse('maybe', Flag), ' ', Flag, ' ', Boolean.TryParse('True', Flag), ' ', Flag);

  // ToHexString without digits: the width of the type
  var Bt: Byte := 7;
  var Wd: Word := $1234;
  var Cd: Cardinal := 10;
  WriteLn(Bt.ToHexString, ' ', Wd.ToHexString, ' ', Cd.ToHexString, ' ', Integer(255).ToHexString,
    ' ', Int64(-2).ToHexString, ' ', Bt.ToHexString(4), ' ', Wd.ToHexString(6));

  // The bytes of a value, the lowest first, as many as SizeOf
  var I: Integer := 258;
  ShowBytes(I.ToBytes);
  I := -2;
  ShowBytes(I.ToBytes);
  var W: Word := $ABCD;
  ShowBytes(W.ToBytes);
  var L: Int64 := Int64.MinValue;
  ShowBytes(L.ToBytes);
  U := UInt64.MaxValue;
  ShowBytes(U.ToBytes);
  ShowBytes(ShortInt(-1).ToBytes);

  // The bits of Single and Double
  var D: Double := 1.5;
  S := -2;
  WriteLn(D.ToBits.ToHexString, ' ', S.ToBits.ToHexString);
  WriteLn(Double.FromBits($4000000000000000), ' ', Single.FromBits($3F800000));
  WriteLn(Double.FromBits($7FF8000000000000).IsNan, ' ', Double.FromBits($7FF0000000000000));
  WriteLn(Double.FromBits(Double.Epsilon.ToBits) = Double.Epsilon, ' ', Double.Epsilon.ToBits);

  // The special values
  WriteLn(Double.MaxValue, ' ', Double.MinValue, ' ', Double.Epsilon);
  WriteLn(Single.MaxValue, ' ', Single.MinValue, ' ', Single.Epsilon);
  WriteLn(Double.NaN, ' ', Double.PositiveInfinity, ' ', Single.NegativeInfinity);
  WriteLn(Double.NaN.IsNan, ' ', Double.NaN.IsInfinity, ' ', Double.NaN = Double.NaN);
  WriteLn(Single.NegativeInfinity.IsInfinity, ' ', Single.NegativeInfinity.IsNegativeInfinity,
    ' ', Single.NegativeInfinity.IsPositiveInfinity);
  WriteLn(Single.Parse('1e39').IsPositiveInfinity, ' ', Double.Parse('-INF').IsNegativeInfinity);

  // Char: Unicode properties, as members of a value and through the type's name
  var C: Char := 'a';
  WriteLn(C.IsLetter, ' ', C.IsLower, ' ', C.IsUpper, ' ', C.IsDigit, ' ', C.ToUpper);
  WriteLn(Char.IsLetter('é'), ' ', Char.IsLetter('漢'), ' ', Char.IsLetter('_'), ' ',
    Char.IsLetter('1'));
  WriteLn(Char.IsDigit('7'), ' ', Char.IsDigit('٣'), ' ', Char.IsDigit('７'), ' ',
    Char.IsDigit('½'), ' ', Char.IsDigit('Ⅳ'));
  WriteLn(Char.IsNumber('7'), ' ', Char.IsNumber('½'), ' ', Char.IsNumber('Ⅳ'), ' ',
    Char.IsNumber('x'));
  WriteLn(Char.IsLetterOrDigit('Ж'), ' ', Char.IsLetterOrDigit('٣'), ' ',
    Char.IsLetterOrDigit('-'));
  WriteLn(Char.IsWhiteSpace(' '), ' ', Char.IsWhiteSpace(#9), ' ', Char.IsWhiteSpace(#$3000),
    ' ', Char.IsWhiteSpace(#$200B), ' ', Char.IsWhiteSpace('a'));
  WriteLn(Char.IsControl(#0), ' ', Char.IsControl(#$7F), ' ', Char.IsControl(#$85), ' ',
    Char.IsControl(' '));
  WriteLn(Char.IsUpper('Ä'), ' ', Char.IsLower('ß'), ' ', Char.IsUpper('1'));
  WriteLn(Char.IsSurrogate(#$D800), ' ', Char.IsHighSurrogate(#$DBFF), ' ',
    Char.IsLowSurrogate(#$DBFF), ' ', Char.IsLowSurrogate(#$DC00), ' ', Char.IsLetter(#$D800));
  WriteLn(Char.ToUpper('é'), ' ', Char.ToLower('Σ'), ' ', Char.ToUpper('ß'), ' ',
    Char.ToUpper('1'), ' ', 'Ω'.ToLower);

  // The reserved word `string` and its alias name the type too
  WriteLn(string.Join(', ', ['a', 'b']), ' ', string.IsNullOrEmpty(''), ' ', TText.Compare('a', 'b'));

  // A subrange type finds the helper of its base type, a distinct type of its original type
  var Digit: TDigit := 9;
  var Count: TCount := 300;
  WriteLn(Digit.ToHexString, ' ', Count.ToHexString(4), ' ', TCount.MaxValue);

  // Uncaught: Parse out of the type's range
  WriteLn(Word.Parse('70000'));
end.
