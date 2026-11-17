program LendFastPaths;

// Lending an element or field to a `var` parameter only stops the slot accesses of the blocks
// holding the lent location (types.md chapter 3): the lent record itself, other blocks and the
// other elements of the array are read and written as usual, while the lent slot and copies of
// a record containing it are refused. The value goes back to the location the root leads to
// when the call ends, also when the root is a captured variable or now holds another array.

type
  TItem = record
    Count, Total: Integer;
  end;
  TPair = record
    X, Y: Integer;
  end;

var
  Items: array of TItem;
  Other: array of TItem;
  K, B, C, Z: array of Integer;
  P, Q: TPair;

procedure Bump(var It: TItem; V: Integer);
begin
  It.Count := It.Count + 1;
  It.Total := It.Total + V;
  Other[0].Count := Other[0].Count + It.Count;
  WriteLn('neighbour: ', Items[0].Count, ' ', Items[2].Total);
  Items[0].Total := 5;
  try
    WriteLn(Items[1].Count);
  except
    on E: EAliasError do
      WriteLn('lent element: ', E.Message);
  end;
end;

procedure Swap(var X, Y: Integer);
var
  T: Integer;
begin
  T := X;
  X := Y;
  Y := T;
end;

procedure SwapCopy(var X, Y: Integer);
begin
  try
    Q := P;
  except
    on E: EAliasError do
      WriteLn('copy: ', E.Message);
  end;
  Swap(X, Y);
end;

procedure Replace(var X: Integer);
begin
  K := B;
  X := 42;
end;

procedure ReplaceEmpty(var X: Integer);
begin
  K := Z;
  X := 43;
end;

procedure Captured;
var
  L: array of Integer;
  Show: TProc;
begin
  L := [10, 20];
  Show := procedure
    begin
      WriteLn('captured: ', L[0], ' ', L[1]);
    end;
  Swap(L[0], L[1]);
  Show();
end;

begin
  SetLength(Items, 3);
  SetLength(Other, 1);
  Items[2].Total := 8;
  Bump(Items[1], 3);
  Bump(Items[1], 4);
  WriteLn(Items[0].Total, ' ', Items[1].Count, ' ', Items[1].Total, ' ', Other[0].Count);
  P.X := 1;
  P.Y := 2;
  Swap(P.X, P.Y);
  SwapCopy(P.X, P.Y);
  WriteLn(P.X, ' ', P.Y, ' ', Q.X, ' ', Q.Y);
  K := [1, 2, 3];
  Swap(K[0], K[2]);
  WriteLn(K[0], ' ', K[1], ' ', K[2]);
  C := K;
  B := [7, 8, 9];
  Replace(K[1]);
  WriteLn(K[1], ' ', B[1], ' ', C[1]);
  C := K;
  Z := [0, 0, 0];
  ReplaceEmpty(K[2]);
  WriteLn(K[2], ' ', Z[2], ' ', C[2]);
  Captured;
end.
