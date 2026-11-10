program DumpEnums;

// The `hir` and `bytecode` dumps of enumeration and subrange types (semantics.md 9,
// bytecode.md 5): the `.type` lines, comparisons by ordinal, `ChkRange` on a store into a
// subrange, and `EnumToStr` with the names in the type section.

type
  TColor = (Red, Green, Blue);
  TCode = (One = 1, Five = 5);
  TDigit = 0..9;
  TWarm = TColor.Red..TColor.Green;

var
  C: TColor;
  D: TDigit;
  W: TWarm;
  N: Integer;

begin
  C := TColor.Green;
  N := 4;
  D := N;
  W := C;
  if C > TColor.Red then
    WriteLn(C, ' ', Succ(D), ' ', TCode.Five);
end.
