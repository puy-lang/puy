program AbsNarrow;

// Abs(ShortInt) is Integer, so storing it back into a ShortInt variable is checked by the assignment's {$R}.
// Under {$R-} it truncates; an out-of-range constant is E0714 (semantics.md 5.1, 2.4)

var
  S: ShortInt;
  B: Byte;
begin
  S := -128;
  S := Abs(S);
  B := Abs(-300);
  WriteLn(S, B);
end.
