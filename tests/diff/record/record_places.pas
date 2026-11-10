program RecordPlaces;

uses System.SysUtils;

// Writing to nested places (fields of elements, elements of fields), passing fields and
// elements to var parameters, multi-dimensional arrays and arrays indexed by Char and
// Boolean (types.md 2.2, chapter 3).

type
  TCell = record
    Value: Integer;
    Tags: array[1..2] of string;
  end;
  TBoard = array[1..3] of TCell;
  TGrid = array[0..1, 0..2] of Integer;
  TCounts = array['a'..'e'] of Integer;
  TFlags = array[Boolean] of string;
  TPair = record
    Key: string;
    Board: TBoard;
  end;

procedure AddTo(var N: Integer; D: Integer);
begin
  N := N + D;
end;

procedure Rename(var S: string);
begin
  S := S + '*';
end;

procedure Clear(out C: TCell);
begin
  C := Default(TCell);
  C.Tags[2] := '-';
end;

var
  B: TBoard;
  G: TGrid;
  C: TCounts;
  F: TFlags;
  P: TPair;
  I, J: Integer;
  Ch: Char;
  S: string;
begin
  for I := 1 to 3 do
  begin
    B[I].Value := I * 10;
    B[I].Tags[1] := 'x' + IntToStr(I);
    B[I].Tags[2] := 'y';
  end;
  AddTo(B[2].Value, 5);
  Rename(B[3].Tags[2]);
  Inc(B[1].Value);
  Dec(B[1].Value, 3);
  for I := 1 to 3 do
    WriteLn(B[I].Value, ' ', B[I].Tags[1], ' ', B[I].Tags[2]);

  for I := 0 to 1 do
    for J := 0 to 2 do
      G[I, J] := I * 3 + J;
  AddTo(G[1][2], 100);
  WriteLn(G[0, 0], ' ', G[0][2], ' ', G[1, 0], ' ', G[1, 2]);

  C := Default(TCounts);
  S := 'abcabcaee';
  for I := 1 to Length(S) do
    Inc(C[S[I]]);
  for Ch := 'a' to 'e' do
    Write(Ch, '=', C[Ch], ' ');
  WriteLn;
  WriteLn(Low(C), ' ', High(C), ' ', Length(C));

  F[False] := 'no';
  F[True] := 'yes';
  WriteLn(F[1 > 2], ' ', F[2 > 1]);

  P.Key := 'k';
  P.Board := B;
  P.Board[2].Tags[1] := 'changed';
  AddTo(P.Board[1].Value, 1000);
  Clear(P.Board[3]);
  WriteLn(P.Board[1].Value, ' ', P.Board[2].Tags[1], ' ', P.Board[3].Value, ' ', P.Board[3].Tags[2], '.');
  WriteLn(B[1].Value, ' ', B[2].Tags[1], ' ', B[3].Value, ' ', B[3].Tags[2]);
end.
