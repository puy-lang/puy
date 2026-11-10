program SliceErrors;

// Diagnostics of slices (types.md 8.7): a slice before the last index (E0720), a slice as an
// assignment target and as the container of one (E0707), as a `var` argument and the target of
// `Inc` (E0706), bounds of the wrong type (E0719), and a value that takes no index (E0720).

type
  TGrid = array[0..1, 0..3] of Integer;
  TColor = (Red, Green, Blue);
  TByColor = array[TColor] of Integer;

procedure Change(var X: string);
begin
  X := 'x';
end;

var
  S: string;
  G: TGrid;
  C: TByColor;
  D: array of Integer;
  N: Integer;
begin
  S := 'abc';
  G := Default(TGrid);
  C := Default(TByColor);
  D := [1, 2];
  N := 1;
  WriteLn(Length(G[0..1, 2]));
  S[1..2] := 'ab';
  D[0..1][0] := 1;
  Change(S[1..2]);
  Inc(D[..1][0]);
  WriteLn(S['a'..]);
  WriteLn(Length(C[0..1]));
  WriteLn(N[1..2]);
end.
