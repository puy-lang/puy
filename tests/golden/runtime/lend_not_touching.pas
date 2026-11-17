program LendNotTouching;

// What does not touch a lent location (types.md chapter 3): copying the reference of the
// dynamic array, its length, resizing it in place while it has one reference, and replacing
// the container. The value goes back to the location found again from the variable after the
// call: a replaced record gets it, and so does an element grown back after the array shrank.
// A record holding the dynamic array whose element is lent can be copied: the copy shares the
// array, so it sees the value given back.

type
  TRec = record
    N, M: Integer;
  end;
  THolder = record
    Items: array of Integer;
  end;

var
  A, B: array of Integer;
  R: TRec;
  H, HCopy: THolder;

// The temporaries that read `A` (`Length(A)`, the condition, the collection of `for..in`) let
// go of it when their statement ends, so `A` has one reference again at `SetLength`
// (lifetime.md 3.4)
procedure Share(var X: Integer);
begin
  B := A;
  WriteLn(Length(A), ' ', Length(B), ' ', B[0]);
  B := nil;
  // Leaves before the lent A[2] is read; the collection is let go of on the `Break` path too
  for var V in A do
  begin
    WriteLn(V);
    if V = 1 then
      Break;
  end;
  if Length(A) = 3 then
    SetLength(A, 5);
  X := 7;
end;

procedure Regrow(var X: Integer);
begin
  SetLength(A, 1);
  while Length(A) < 3 do
    SetLength(A, Length(A) + 1);
  WriteLn(A[2]);
  A[2] := 4;
  X := 8;
end;

procedure Replace(var X: Integer);
begin
  X := 6;
  R := Default(TRec);
  R.M := 9;
end;

procedure CopyHolder(var X: Integer);
begin
  HCopy := H;
  X := 3;
end;

begin
  SetLength(A, 3);
  A[0] := 1;
  Share(A[2]);
  WriteLn(Length(A), ' ', A[2]);
  Regrow(A[2]);
  WriteLn(Length(A), ' ', A[2]);
  R.N := 1;
  R.M := 2;
  Replace(R.N);
  WriteLn(R.N, ' ', R.M);
  SetLength(H.Items, 1);
  CopyHolder(H.Items[0]);
  WriteLn(H.Items[0], ' ', HCopy.Items[0]);
end.
