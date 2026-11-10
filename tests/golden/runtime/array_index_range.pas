program ArrayIndexRange;

// An element index outside the bounds is always an ERangeError, whatever `{$R}` says
// (types.md 2.2). The message shows the index as written and the type's bounds.

{$R-}

type
  TCounts = array['a'..'c'] of Integer;
  TRow = array[-1..1] of Integer;

var
  C: TCounts;
  R: TRow;
  Ch: Char;
  I: Integer;
begin
  C := Default(TCounts);
  R := Default(TRow);
  Ch := 'b';
  Inc(C[Ch]);
  WriteLn(C['b']);
  I := -1;
  R[I] := 5;
  WriteLn(R[-1]);
  I := 2;
  WriteLn(R[I]);
end.
