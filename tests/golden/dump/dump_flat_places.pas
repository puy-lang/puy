program DumpFlatPlaces;

// Records and static arrays laid out flat in their containers (bytecode.md 1.5, 2.8;
// docs/design/records.md): an element's field through a cursor (`ElemAt`, `SlotGet` /
// `SlotSet`), a whole element (`AggLoad` / `AggStore`), a nested field folded into one slot
// number (`FieldSet`, `ObjSet`), a variable index into a flat static array (`MulIntI` and
// `ValGet`), a flat element lent to a `var` parameter (`LendPlace`) or to a `var` open array
// parameter (`PlaceBox`), and a field read and written through a reference to a flat place
// (`RefFieldGet` / `RefFieldSet`).

type
  TPoint = record
    X, Y: Integer;
  end;
  TLine = record
    A, B: TPoint;
    N: array[0..2] of Integer;
  end;
  THolder = class
    L: TLine;
  end;

var
  Pts: array of TPoint;
  L: TLine;
  H: THolder;
  I: Integer;

procedure Bump(var P: TPoint);
begin
  P.X := P.X + 1;
end;

procedure Fill(var V: array of Integer);
begin
  V[0] := 1;
end;

begin
  SetLength(Pts, 2);
  I := 1;
  Pts[I].Y := 2;
  Pts[0] := Pts[I];
  L.B.X := Pts[0].Y;
  L.N[I] := L.B.X;
  H := THolder.Create;
  H.L.B.Y := L.N[I];
  Bump(Pts[I]);
  Fill(L.N);
  WriteLn(Pts[1].X, ' ', H.L.B.Y, ' ', L.N[0]);
  H.Free;
end.
