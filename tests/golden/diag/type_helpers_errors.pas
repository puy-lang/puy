program TypeHelpersErrors;

// The diagnostics of the helpers of the integer types, the floating-point types, Boolean and Char
// (spec/stdlib/System/Helpers.md): a name they do not have is E0700, on a value and through the
// type's name; an instance member through the type's name is E0700; a constant cannot be
// assigned (E0707); `TCharHelper.IsDigit(C)` matches both the static member and the instance
// member called with its receiver first (E0734); the built-ins the helpers use are not visible
// (E0700); and Currency and UTF8String, whose helpers have no members yet, keep the E3200.

var
  I: Integer;
  D: Double;
  C: Char;
  M: Currency;
begin
  I := 1;
  D := 1;
  C := 'a';
  M := 1;
  WriteLn(I.Foo);                    // E0700
  WriteLn(D.IsFinite);               // E0700
  WriteLn(Integer.Foo);              // E0700
  WriteLn(Boolean.Bar(True));        // E0700
  WriteLn(Integer.ToHexString);      // E0700: an instance member
  WriteLn(Char.IsDigit);             // E0700: an instance member (the static one takes C)
  WriteLn(Double.ToString);          // E0700: the built-in ToString needs a value
  WriteLn(string.Trim);              // E0700: an instance member
  WriteLn(string.Empty);             // E0700
  WriteLn(UTF8String.Foo);           // E3200
  Integer.MaxValue := 3;             // E0707
  WriteLn(TCharHelper.IsDigit(C));   // E0734
  WriteLn(Integer.Parse(1));         // E0702
  WriteLn(CharClass(C));             // E0700: only the standard library sees it
  WriteLn(SingleToBits(1.5));        // E0700
  WriteLn(M.ToHexString);            // E3200
  WriteLn(Currency.Parse('1'));      // E3200
end.
