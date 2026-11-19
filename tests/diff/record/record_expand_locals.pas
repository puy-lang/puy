program RecordExpandLocals;

// Local records the compiler keeps in one register per field (docs/design/records.md 3.9):
// copies to and from array elements, record fields, static array elements and instance
// fields, between two such locals and from Default; their fields read, written, passed to var
// parameters, incremented and appended to; a record field that is a block of its own; and an
// element read that raises, which leaves the local as it was.

uses
  System.SysUtils;

type
  TPoint = record
    X, Y: Integer;
  end;
  TLine = record
    A, B: TPoint;
    Tag: string;
  end;
  TBig = record
    V: array[0..19] of Integer;
    Name: string;
  end;
  TWrap = record
    Big: TBig;
    P: TPoint;
  end;
  TPoints = array[0..2] of TPoint;
  THolder = class
    Pt: TPoint;
  end;
  TBox = record
    Line: TLine;
    N: Integer;
  end;

var
  GBox: TBox;

procedure Swap(var A, B: Integer);
var
  T: Integer;
begin
  T := A;
  A := B;
  B := T;
end;

procedure AddTo(var S: string; const X: string);
begin
  S := S + X;
end;

procedure Show(const Name: string; const P: TPoint);
begin
  WriteLn(Name, ' ', P.X, ' ', P.Y);
end;

procedure Elements;
var
  A: array of TPoint;
  P, Q: TPoint;
  I, Sum: Integer;
begin
  P := Default(TPoint);
  SetLength(A, 4);
  for I := 0 to 3 do
  begin
    P.X := I;
    P.Y := I * 10;
    A[I] := P;
  end;
  Sum := 0;
  for I := 0 to 3 do
  begin
    P := A[I];
    P.X := P.X + 100;
    Sum := Sum + P.X + A[I].X;
  end;
  WriteLn('sum ', Sum);
  Show('a2', A[2]);
  Q := P;
  Q.Y := -1;
  WriteLn('p ', P.X, ' ', P.Y, ' q ', Q.X, ' ', Q.Y);
  Swap(Q.X, Q.Y);
  Inc(Q.X, 5);
  Dec(Q.Y);
  A[0] := Q;
  Show('a0', A[0]);
  Q := Default(TPoint);
  WriteLn('q ', Q.X, ' ', Q.Y);
end;

procedure Nested;
var
  L, M: TLine;
  P: TPoint;
begin
  L := Default(TLine);
  L.A.X := 1;
  L.A.Y := 2;
  L.B := L.A;
  L.B.Y := 20;
  L.Tag := 'line';
  L.Tag := L.Tag + '!';
  AddTo(L.Tag, '?');
  L.Tag[1] := 'L';
  P := L.B;
  M := L;
  M.Tag := M.Tag + 'm';
  M.A := P;
  M.A.X := 7;
  WriteLn(L.A.X, ' ', L.A.Y, ' ', L.B.X, ' ', L.B.Y, ' ', L.Tag);
  WriteLn(M.A.X, ' ', M.A.Y, ' ', M.B.X, ' ', M.B.Y, ' ', M.Tag);
  WriteLn(P.X, ' ', P.Y, ' ', Length(M.Tag));
  GBox.Line := M;
  GBox.Line.Tag := 'global';
  L := GBox.Line;
  WriteLn(L.A.X, ' ', L.Tag, ' ', M.Tag);
  M := Default(TLine);
  WriteLn(M.A.X, ' ', M.B.Y, ' [', M.Tag, ']');
end;

procedure Containers;
var
  S: TPoints;
  H: THolder;
  P: TPoint;
  W, W2: TWrap;
  K: Integer;
begin
  S := Default(TPoints);
  P := Default(TPoint);
  P.X := 3;
  P.Y := 4;
  S[1] := P;
  P := S[0];
  WriteLn('s ', S[1].X, ' ', S[1].Y, ' p ', P.X, ' ', P.Y);
  K := 1;
  P := S[K];
  P.Y := 40;
  S[K + 1] := P;
  WriteLn('s2 ', S[2].X, ' ', S[2].Y, ' s1 ', S[1].Y);
  H := THolder.Create;
  try
    H.Pt := P;
    P.X := 0;
    WriteLn('h ', H.Pt.X, ' ', H.Pt.Y);
    P := H.Pt;
    WriteLn('p ', P.X, ' ', P.Y);
  finally
    H.Free;
  end;
  W := Default(TWrap);
  W.Big.Name := 'big';
  W.Big.V[3] := 33;
  W.P.X := 9;
  W2 := W;
  W2.Big.V[3] := 34;
  W2.Big.Name := W2.Big.Name + '2';
  W2.P.X := 10;
  WriteLn(W.Big.V[3], ' ', W.Big.Name, ' ', W.P.X);
  WriteLn(W2.Big.V[3], ' ', W2.Big.Name, ' ', W2.P.X);
end;

procedure Raising;
var
  A: array of TPoint;
  P: TPoint;
  I: Integer;
begin
  SetLength(A, 2);
  A[1].X := 5;
  A[1].Y := 6;
  P := A[1];
  I := 2;
  try
    P := A[I];
  except
    on E: ERangeError do
      WriteLn('range error');
  end;
  WriteLn('p ', P.X, ' ', P.Y);
end;

procedure Whole;
var
  P: TPoint;
begin
  // Passed whole: the record keeps its block
  P := Default(TPoint);
  P.X := 1;
  P.Y := 2;
  Show('whole', P);
end;

begin
  Elements;
  Nested;
  Containers;
  Raising;
  Whole;
end.
