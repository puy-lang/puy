program RecordFlatPlaces;

// Records and static arrays kept inside arrays, records and instances (laid out flat there;
// docs/design/records.md): whole reads and writes, nested fields with variable indices,
// var parameters taking such a place (the callee writes it in place), and copies staying
// independent of the place they were read from.

uses
  System.SysUtils;

type
  TPoint = record
    X, Y: Integer;
  end;
  TTriple = array[0..2] of Integer;
  TLine = record
    A, B: TPoint;
    Tag: string;
    Nums: TTriple;
  end;
  TBig = record
    V: array[0..19] of Integer;
    Name: string;
  end;
  TWrap = record
    Big: TBig;
    P: TPoint;
  end;
  TGrid = array[0..1] of array[0..1] of TPoint;
  THolder = class
    P: TPoint;
    L: TLine;
    Grid: TGrid;
    Nums: TTriple;
  end;

var
  Pts: array of TPoint;
  Lines: array of TLine;
  Wraps: array of TWrap;
  G: TGrid;
  L, M: TLine;
  P: TPoint;
  H: THolder;
  I, J, K: Integer;

procedure Bump(var P: TPoint);
begin
  P.X := P.X + 10;
  Inc(P.Y);
end;

procedure SwapInt(var A, B: Integer);
var
  T: Integer;
begin
  T := A;
  A := B;
  B := T;
end;

procedure Fill(var A: array of Integer; Base: Integer);
var
  N: Integer;
begin
  for N := 0 to High(A) do
    A[N] := Base + N;
end;

procedure EditLine(var L: TLine; N: Integer);
var
  Copy: TPoint;
begin
  Copy := L.A;
  L.A.X := L.A.X + 1;
  L.B := Copy;
  L.Tag := L.Tag + '+';
  L.Nums[N] := L.Nums[N] * 2;
  Bump(L.B);
  SwapInt(L.Nums[0], L.Nums[2]);
  Fill(L.Nums, N * 100);
end;

procedure EditTriple(var T: TTriple; N: Integer);
begin
  T[N] := T[N] + 7;
  SwapInt(T[0], T[N]);
end;

procedure Forward(var T: TTriple);
begin
  Fill(T, 40);
  EditTriple(T, 2);
end;

procedure Reset(out T: TTriple);
begin
  T := Default(TTriple);
  T[1] := 1;
end;

procedure EditGrid(var G: TGrid; R, C: Integer);
begin
  G[R][C].X := G[R][C].X + 5;
  Bump(G[R][C]);
  G[C][R] := G[R][C];
end;

procedure EditWrap(var W: TWrap; N: Integer);
begin
  W.Big.V[N] := W.Big.V[N] + 3;
  W.Big.Name := W.Big.Name + '!';
  W.P.Y := N;
end;

procedure Fail(var P: TPoint);
begin
  P.X := 77;
  raise Exception.Create('fail');
end;

function Total(const L: TLine): Integer;
begin
  Result := L.A.X + L.A.Y + L.B.X + L.B.Y + L.Nums[0] + L.Nums[1] + L.Nums[2];
end;

procedure ShowLine(const L: TLine);
begin
  WriteLn(L.A.X, ' ', L.A.Y, ' ', L.B.X, ' ', L.B.Y, ' ', L.Tag, ' ', L.Nums[0], ' ',
    L.Nums[1], ' ', L.Nums[2]);
end;

begin
  SetLength(Pts, 3);
  for I := 0 to 2 do
  begin
    Pts[I].X := I;
    Pts[I].Y := I * 10;
  end;
  P := Pts[1];
  P.X := 100;
  WriteLn(Pts[1].X, ' ', P.X);
  Pts[0] := P;
  P.Y := -1;
  WriteLn(Pts[0].X, ' ', Pts[0].Y);
  Bump(Pts[2]);
  I := 2;
  SwapInt(Pts[I].X, Pts[I].Y);
  WriteLn(Pts[2].X, ' ', Pts[2].Y);
  for P in Pts do
    Write(P.X, ':', P.Y, ' ');
  WriteLn;

  L.A.X := 1;
  L.A.Y := 2;
  L.Tag := 'l';
  L.Nums[1] := 5;
  M := L;
  M.A.X := 9;
  M.Nums[1] := 6;
  ShowLine(L);
  ShowLine(M);
  SetLength(Lines, 2);
  Lines[1] := L;
  J := 1;
  EditLine(Lines[J], 1);
  ShowLine(Lines[1]);
  ShowLine(L);
  Lines[0] := Lines[1];
  Lines[1].Nums[2] := -5;
  WriteLn(Lines[0].Nums[2], ' ', Lines[1].Nums[2], ' ', Total(Lines[0]));
  EditTriple(Lines[J].Nums, 1);
  ShowLine(Lines[1]);
  K := 2;
  Lines[J].Nums[K] := Lines[J].Nums[K - 1] + Lines[0].A.Y;
  WriteLn(Lines[1].Nums[2]);
  Forward(Lines[J].Nums);
  ShowLine(Lines[1]);
  Reset(Lines[0].Nums);
  ShowLine(Lines[0]);

  G[1][0].X := 4;
  EditGrid(G, 1, 0);
  for I := 0 to 1 do
    for J := 0 to 1 do
      Write(G[I][J].X, ',', G[I][J].Y, ' ');
  WriteLn;

  SetLength(Wraps, 2);
  Wraps[0].Big.Name := 'w';
  EditWrap(Wraps[0], 4);
  Wraps[1] := Wraps[0];
  EditWrap(Wraps[1], 4);
  WriteLn(Wraps[0].Big.V[4], ' ', Wraps[0].Big.Name, ' ', Wraps[1].Big.V[4], ' ',
    Wraps[1].Big.Name, ' ', Wraps[1].P.Y);

  H := THolder.Create;
  H.P.X := 3;
  Bump(H.P);
  H.L := L;
  H.L.B.Y := 42;
  EditLine(H.L, 2);
  Fill(H.Nums, 7);
  I := 1;
  H.Grid[I][I] := H.P;
  EditGrid(H.Grid, 1, 1);
  WriteLn(H.P.X, ' ', H.P.Y, ' ', H.Nums[2], ' ', H.Grid[1][1].X, ' ', H.Grid[1][1].Y);
  ShowLine(H.L);
  ShowLine(L);
  P := H.P;
  H.P.X := 0;
  WriteLn(P.X);

  try
    Fail(Pts[1]);
  except
    on E: Exception do
      WriteLn(E.Message, ' ', Pts[1].X);
  end;
  try
    Fail(H.P);
  except
    on E: Exception do
      WriteLn(E.Message, ' ', H.P.X);
  end;
  H.Free;

  Pts := Copy(Pts, 1, 2);
  WriteLn(Length(Pts), ' ', Pts[0].X, ' ', Pts[1].Y);
  SetLength(Pts, 3);
  Pts[2] := Pts[1];
  Pts[1] := P;
  Pts := Copy(Pts, 1, 2);
  WriteLn(Length(Pts), ' ', Pts[0].X, ' ', Pts[1].Y);
  SetLength(Pts, 4);
  WriteLn(Pts[3].X, ' ', Pts[3].Y);
end.
