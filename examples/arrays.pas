program Arrays;

// Static arrays with subrange and multi-dimensional indexes, typed array
// constants, slices, and Insert / Delete on dynamic arrays.

type
  TSize = 1..3;
  TMatrix = array[TSize, TSize] of Integer;
  TMonth = 1..12;

const
  DaysIn: array[TMonth] of Integer = (31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31);
  Identity: TMatrix = ((1, 0, 0), (0, 1, 0), (0, 0, 1));

function Multiply(const A, B: TMatrix): TMatrix;
begin
  Result := Default(TMatrix);
  for var I := Low(TSize) to High(TSize) do
    for var J := Low(TSize) to High(TSize) do
    begin
      var Sum := 0;
      for var K := Low(TSize) to High(TSize) do
        Sum := Sum + A[I, K] * B[K, J];
      Result[I, J] := Sum;
    end;
end;

procedure Print(const M: TMatrix);
begin
  for var I := Low(TSize) to High(TSize) do
  begin
    for var J := Low(TSize) to High(TSize) do
      Write(M[I, J]:4);
    WriteLn;
  end;
end;

procedure Show(const Title: string; const Values: array of Integer);
begin
  Write(Title, ':');
  for var V in Values do
    Write(' ', V);
  WriteLn;
end;

var
  M: TMatrix;
  Days: TArray<Integer>;

begin
  M := Identity;
  M[1, 3] := 2;
  M[3, 1] := -1;
  Print(Multiply(M, M));

  // Slices of a static array use its own indexes (here 1..12) and give a
  // dynamic array. Out-of-range bounds are clipped instead of raising.
  var Total := 0;
  for var D in DaysIn[1..6] do
    Total := Total + D;
  WriteLn('Days in the first half: ', Total);
  Show('Last quarter', DaysIn[10..99]);

  Days := DaysIn[..3];
  Insert(29, Days, 2);
  Delete(Days, 0, 1);
  Show('Edited', Days);
  Show('Joined', Days + DaysIn[12..]);
  WriteLn('''December''[6..] = ', 'December'[6..]);
end.
