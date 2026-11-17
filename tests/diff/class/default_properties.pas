program DefaultProperties;

// Default array properties (objects.md 12.1): `X[I, J]` reads and writes a two-dimensional
// property of a class, `X.Cells[…]` names the same property, a record's default property takes a
// string index, `X[I][J]` applies the default property of the value `X[I]` returns, and
// `O.Grid[I, J]` the one of the value a property without indices returns. (The order
// of evaluation is in golden/runtime/default_property_overloads: FPC evaluates from the right.)

type
  TGrid = class
  private
    FData: array[0..2, 0..2] of Integer;
    function GetCell(X, Y: Integer): Integer;
    procedure SetCell(X, Y: Integer; Value: Integer);
  public
    property Cells[X, Y: Integer]: Integer read GetCell write SetCell; default;
  end;

  TDict = record
    Keys: array[0..1] of string;
    function GetValue(const K: string): Integer;
    property Values[const K: string]: Integer read GetValue; default;
  end;

  TRows = class
    function GetRow(I: Integer): TGrid;
    property Rows[I: Integer]: TGrid read GetRow; default;
  end;

  THolder = class
  private
    FGrid: TGrid;
  public
    property Grid: TGrid read FGrid write FGrid;
  end;

var
  Shared: TGrid;

function TGrid.GetCell(X, Y: Integer): Integer;
begin
  Result := FData[X, Y];
end;

procedure TGrid.SetCell(X, Y: Integer; Value: Integer);
begin
  WriteLn('set ', X, ' ', Y, ' ', Value);
  FData[X, Y] := Value;
end;

function TDict.GetValue(const K: string): Integer;
begin
  if Keys[0] = K then
    Result := 0
  else if Keys[1] = K then
    Result := 1
  else
    Result := -1;
end;

function TRows.GetRow(I: Integer): TGrid;
begin
  WriteLn('row ', I);
  Result := Shared;
end;

var
  G: TGrid;
  D: TDict;
  R: TRows;
  H: THolder;
begin
  G := TGrid.Create;
  Shared := G;
  G[1, 2] := 7;
  G[0, 0] := G[1, 2] + 1;
  WriteLn(G[0, 0], ' ', G.Cells[1, 2]);
  D.Keys[0] := 'x';
  D.Keys[1] := 'y';
  WriteLn(D['y'], ' ', D['q']);
  R := TRows.Create;
  R[3][1, 1] := 4;
  WriteLn(R[5][1, 1]);
  H := THolder.Create;
  H.Grid := G;
  H.Grid[2, 2] := 6;
  WriteLn(H.Grid[2, 2], ' ', G[2, 2]);
  H.Free;
  R.Free;
  G.Free;
end.
