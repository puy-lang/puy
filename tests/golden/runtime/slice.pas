program Slice;

// Slices `X[L..H]` (types.md 8.7): strings, dynamic arrays, a static array whose low bound is
// not 0, static arrays indexed by an enumeration and by `Char`, open array parameters, the
// forms with a bound left out, bounds cut to the sequence, `L > H`, the last index of a
// multidimensional array, a bound of a subrange type, and a slice as the source of an element,
// a `for..in` and `Length`.
// Not in FPC.

type
  TInts = array of Integer;
  TFive = array[5..9] of Integer;
  TGrid = array[0..1, 0..3] of Integer;
  TColor = (Red, Green, Blue);
  TByColor = array[TColor] of string;
  TCool = TColor.Green..TColor.Blue;

procedure Show(const Title: string; const A: array of Integer);
var
  I: Integer;
begin
  Write(Title, ': [');
  for I := 0 to High(A) do
  begin
    if I > 0 then
      Write(', ');
    Write(A[I]);
  end;
  WriteLn('] (', Length(A), ')');
end;

procedure OpenSlices(const A: array of Integer);
begin
  Show('open 1..2', A[1..2]);
  Show('open ..0', A[..0]);
  Show('open 3..', A[3..]);
end;

function Middle(const S: string): string;
begin
  Result := S[2..Length(S) - 1];
end;

var
  S: string;
  D: TInts;
  F: TFive;
  G: TGrid;
  C: TByColor;
  B: array['a'..'e'] of Integer;
  I, L, H: Integer;
  Big: Int64;
  Cool: TCool;
begin
  S := 'Hello, world';
  WriteLn('[', S[1..5], ']');
  WriteLn('[', S[8..], ']');
  WriteLn('[', S[..5], ']');
  WriteLn('[', S[..], ']');
  WriteLn('[', S[-3..2], ']');
  WriteLn('[', S[10..100], ']');
  WriteLn('[', S[5..3], ']');
  WriteLn('[', S[100..200], ']');
  WriteLn('[', S[0..0], ']');
  WriteLn('[', Middle('(abc)'), ']');
  WriteLn(S[8..][1], ' ', Length(S[3..4]));
  Big := High(Int64);
  WriteLn('[', S[-Big - 1..Big], ']');
  WriteLn('[', S[Big..-Big - 1], ']');

  D := [10, 20, 30, 40, 50];
  Show('dyn 1..3', D[1..3]);
  Show('dyn 3..', D[3..]);
  Show('dyn ..1', D[..1]);
  Show('dyn ..', D[..]);
  Show('dyn -5..100', D[-5..100]);
  Show('dyn 4..2', D[4..2]);
  Show('dyn 9..12', D[9..12]);
  D := D[1..3];
  Show('dyn reassigned', D);
  D[0] := 99;
  Show('dyn copy', D[0..0]);
  D := nil;
  Show('nil 0..5', D[0..5]);

  for I := 5 to 9 do
    F[I] := I * 2;
  Show('static 6..7', F[6..7]);
  Show('static ..6', F[..6]);
  Show('static 8..', F[8..]);
  Show('static 0..100', F[0..100]);
  Show('static 0..4', F[0..4]);

  G := Default(TGrid);
  G[1, 2] := 7;
  Show('grid 1, 1..3', G[1, 1..3]);
  Show('grid [1][2..]', G[1][2..]);

  C[TColor.Red] := 'r';
  C[TColor.Green] := 'g';
  C[TColor.Blue] := 'b';
  for S in C[TColor.Green..] do
    Write(S);
  WriteLn(' ', Length(C[..TColor.Green]));
  Cool := TColor.Blue;
  WriteLn(Length(C[Cool..]), Length(C[..Cool]));

  for I := 0 to 4 do
    B[Chr(Ord('a') + I)] := I + 1;
  Show('char b..d', B['b'..'d']);

  OpenSlices([1, 2, 3, 4, 5]);
  OpenSlices(F);

  L := 2;
  H := 3;
  WriteLn('[', 'abcdef'[L..H + 1], ']');
end.
