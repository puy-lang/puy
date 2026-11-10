program SetErrors;

// Errors of sets (types.md 8.3): a base type with ordinals outside 0..65535 (E0737), an
// integer element outside 0..65535 in a set whose type is not written (the E0737 variant),
// two constructors without a set type (E0729), sets whose base types have different bases
// (E0703), `<` on sets, `in` with a left operand of another base, a constant element out of
// the base type's range (E0714, naming the set type), an element of the wrong type (E0702),
// `Include` / `Exclude` of a non-variable (E0706) or a non-set, and `Write` (E0718).

type
  TColor = (Red, Green, Blue);
  TColors = set of TColor;
  TDigit = 0..9;
  TDigits = set of TDigit;
  TBig = (Small = 0, Huge = 70000);
  TBad1 = set of Integer;
  TBad2 = set of ShortInt;
  TBad3 = set of TBig;
  TBad4 = set of -1..5;
  TBad5 = set of string;

const
  Wide = [1, 70000];

var
  S: TColors;
  D: TDigits;
  C: set of Char;
  I: Integer;
begin
  if 5 in [1, 70000] then
    WriteLn;
  if [1] = [1] then
    WriteLn;
  S := [TColor.Red] + [TColor.Blue];
  D := D + C;
  if D = C then
    WriteLn;
  if S < S then
    WriteLn;
  if 'a' in D then
    WriteLn;
  D := [1, 12];
  S := [1];
  Include(D, 15);
  Include(3, 1);
  Include(I, 1);
  Exclude(D, TColor.Red);
  Include(D);
  WriteLn(S);
end.
