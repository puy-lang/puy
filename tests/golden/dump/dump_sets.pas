program DumpSets;

// The `hir` and `bytecode` dumps of sets (semantics.md 9, bytecode.md 5): the `.type` lines,
// a small set as integer bits (`SetRange`, `OrInt`, `SetIn`), a big set as a block (`.type`
// `set` with its number of bits, the set constant, `Unique` + `SetIncl`, `SetInBig`,
// `SetUnion`), `Include` with a range check, a conversion between set types and `for..in`.

{$R+}

type
  TColor = (Red, Green, Blue);
  TColors = set of TColor;
  TChars = set of Char;

const
  Vowels = ['a', 'e', 'i', 'o', 'u'];

var
  S: TColors;
  C: TChars;
  B: set of Byte;
  X: TColor;
  I: Integer;
  Ch: Char;

begin
  X := TColor.Blue;
  S := [TColor.Red, X];
  Ch := 'z';
  C := Vowels + ['x'..Ch];
  I := 200;
  Include(B, I);
  C := C + Vowels;
  if (X in S) and ('y' in C) then
    for X in S do
      WriteLn(X);
  WriteLn(S <= [TColor.Red..TColor.Blue], ' ', C = Vowels);
end.
