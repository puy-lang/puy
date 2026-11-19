program DumpExpandLocals;

// Local records kept in one register per field (docs/design/records.md 3.9; the `expanded` and
// `field` marks of the `ir` dump, bytecode.md 5.1): fields read and written as registers, a
// field passed to a var parameter, an element read and written slot by slot (`SlotGet` /
// `SlotSet`), a copy between two expanded records and of a record field of one, `Default`
// clearing the fields, an instance's field copied slot by slot, and a record passed whole,
// which keeps its block (so would one copied to or from it whole). `Kept` and `Captured` have
// one local record for each of the other conditions that keep a record's block: a copy of a
// record that is not expanded, a `for..in` variable, a record a nested routine uses, a record
// that may hold an instance, one with a `[weak]` field, one with a static array laid out flat,
// and any record of a routine whose variables an anonymous method captures.

type
  TPoint = record
    X, Y: Integer;
  end;
  TLine = record
    A, B: TPoint;
    Tag: string;
  end;
  THolder = class
    Pt: TPoint;
  end;
  TItem = class(TRefObject)
  end;
  TOwned = record
    Item: TItem;
    N: Integer;
  end;
  TBack = record
    [weak] Owner: TItem;
    N: Integer;
  end;
  TNums = record
    V: array[0..1] of Integer;
    N: Integer;
  end;

procedure Twice(var N: Integer);
begin
  N := N * 2;
end;

procedure Show(const P: TPoint);
begin
  WriteLn(P.X, ' ', P.Y);
end;

procedure Run(const A: array of TPoint; H: THolder);
var
  P: TPoint;
  L: TLine;
  W: TPoint;
begin
  P := A[1];
  P.X := P.X + 1;
  Twice(P.Y);
  L := Default(TLine);
  L.A := P;
  L.Tag := 'a';
  L.B := L.A;
  H.Pt := L.B;
  P := H.Pt;
  W := Default(TPoint);
  W.X := P.X;
  Show(W);
  WriteLn(L.A.X, ' ', L.B.Y, ' ', L.Tag);
end;

procedure Kept(const A: array of TPoint);
var
  P, Q: TPoint; // Q is passed whole, so P (copied from Q) is not expanded either
  E: TPoint; // a for..in variable
  N: TPoint; // used by a nested routine
  O: TOwned;
  B: TBack;
  S: TNums;

  procedure Bump;
  begin
    N.X := N.X + 1;
  end;

begin
  Q := A[0];
  Show(Q);
  P := Q;
  WriteLn(P.X);
  for E in A do
    WriteLn(E.X);
  N := Default(TPoint);
  Bump;
  WriteLn(N.X);
  O := Default(TOwned);
  O.N := 1;
  B := Default(TBack);
  B.N := 2;
  S := Default(TNums);
  S.V[1] := 3;
  WriteLn(O.N, ' ', B.N, ' ', S.V[1]);
end;

procedure Captured;
var
  K: Integer;
  P: TPoint;
  F: TProc;
begin
  K := 0;
  F := procedure begin K := K + 1; end;
  F();
  P := Default(TPoint);
  P.X := K;
  WriteLn(P.X);
end;

var
  A: array of TPoint;
  H: THolder;
begin
  SetLength(A, 2);
  A[1].X := 1;
  A[1].Y := 2;
  H := THolder.Create;
  Run(A, H);
  Kept(A);
  Captured;
  H.Free;
end.
