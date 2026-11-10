program DynSharing;

// Dynamic arrays share their elements by reference (types.md 2.3): assignment, value
// parameters, record fields and elements see the same array; `nil` and `Assigned`.

type
  TInts = array of Integer;
  THolder = record
    Name: string;
    Data: TInts;
  end;

procedure Poke(A: TInts; I, V: Integer);
begin
  A[I] := V;
end;

procedure Show(const Title: string; const A: TInts);
var
  I: Integer;
begin
  Write(Title, ':');
  for I := 0 to High(A) do
    Write(' ', A[I]);
  WriteLn(' (', Length(A), ')');
end;

function Make(N: Integer): TInts;
var
  I: Integer;
begin
  SetLength(Result, N);
  for I := 0 to N - 1 do
    Result[I] := I * I;
end;

var
  A, B: TInts;
  C: TArray<Integer>;
  H, H2: THolder;
  Names: array of string;
  Grid: array of TInts;
begin
  A := Make(4);
  B := A;
  B[1] := 100;
  Show('A', A);
  Poke(A, 2, 200);
  Show('B', B);
  // `array of Integer`, `TInts` and `TArray<Integer>` are the same type (types.md 1.2)
  C := A;
  C[0] := -1;
  Show('A', A);
  H.Data := A;
  H2 := H;
  H2.Data[3] := 300;
  Show('A', A);
  H2.Name := 'copy';
  WriteLn('[', H.Name, '] [', H2.Name, ']');
  SetLength(Names, 2);
  Names[0] := 'x';
  Names[1] := Names[0] + 'y';
  WriteLn(Names[0], Names[1]);
  SetLength(Grid, 2);
  Grid[0] := A;
  Grid[1] := Grid[0];
  Grid[1][0] := 7;
  Show('A', A);
  WriteLn(A = nil, ' ', Assigned(A), ' ', B <> nil);
  A := nil;
  WriteLn(A = nil, ' ', Assigned(A), ' ', Length(A), ' ', High(A), ' ', Low(A));
  Show('B', B);
  B := [];
  WriteLn(Length(B), ' ', B = nil);
end.
