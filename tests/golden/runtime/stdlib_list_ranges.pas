program StdlibListRanges;

// The members of TList<T> that take or give several items (collections.md 5.1, 5.4): the
// constructors, AddRange / InsertRange of an open array and of a TEnumerable<T> (the list itself
// included: its items as they were at the call), slices with the rules of a dynamic array,
// ToArray, Extract / ExtractAt, writing Count, and Capacity / TrimExcess. The constructors take
// exactly the capacity of the items (3.5).

uses System.SysUtils, System.Generics.Collections;

procedure Show(const Title: string; const A: array of Integer);
var
  X: Integer;
begin
  Write(Title, ':');
  for X in A do
    Write(' ', X);
  WriteLn(' (', Length(A), ')');
end;

var
  L, M: TList<Integer>;
  S: TList<string>;
  A: TArray<Integer>;
  R: TIndexRange;
begin
  L := TList<Integer>.Create([1, 2, 3, 4, 5]);
  Show('Create(array)', L.ToArray);
  WriteLn('capacity ', L.Capacity);

  WriteLn('slices');
  Show('L[1..3]', L[1..3]);
  Show('L[3..]', L[3..]);
  Show('L[..1]', L[..1]);
  Show('L[..]', L[..]);
  Show('L[-5..1]', L[-5..1]);
  Show('L[3..99]', L[3..99]);
  Show('L[4..2]', L[4..2]);
  Show('L[7..9]', L[7..9]);
  R := Default(TIndexRange);
  R.First := 2;
  R.HasFirst := True;
  Show('L[R]', L[R]);
  M := TList<Integer>.Create(L[1..3]);
  Show('Create(L[1..3])', M.ToArray);

  WriteLn('a slice and ToArray are copies');
  A := L[0..1];
  A[0] := 100;
  A := L.ToArray;
  A[1] := 200;
  Show('L', L.ToArray);

  WriteLn('ranges');
  L.AddRange(M);
  Show('AddRange(M)', L.ToArray);
  L.AddRange(L);
  Show('AddRange(L)', L.ToArray);
  L.Clear;
  L.AddRange([1, 2, 3]);
  L.InsertRange(1, [7, 8]);
  Show('InsertRange(1, [7, 8])', L.ToArray);
  L.InsertRange(0, L);
  Show('InsertRange(0, L)', L.ToArray);
  L.InsertRange(L.Count, [9]);
  Show('InsertRange(Count, [9])', L.ToArray);
  L.DeleteRange(2, 6);
  Show('DeleteRange(2, 6)', L.ToArray);
  M.Free;
  M := TList<Integer>.Create(L);
  Show('Create(L)', M.ToArray);
  WriteLn('capacity ', M.Capacity);
  M.Free;

  WriteLn('Extract');
  L.AddRange([1, 2, 1]);
  Show('L', L.ToArray);
  WriteLn(L.Extract(1), ' ', L.Extract(42), ' ', L.ExtractAt(0));
  Show('L', L.ToArray);

  WriteLn('Count');
  S := TList<string>.Create(['a', 'b', 'c']);
  S.Count := 5;
  WriteLn(S.Count, ' [', S[3], '] [', S[4], ']');
  S.Count := 1;
  S.Count := 2;
  WriteLn(S.Count, ' [', S[0], '] [', S[1], ']');
  S.Free;

  WriteLn('Capacity');
  L.Clear;
  WriteLn(L.Count, ' ', L.Capacity);
  L.TrimExcess;
  WriteLn(L.Count, ' ', L.Capacity);
  L.Capacity := 10;
  L.Add(1);
  WriteLn(L.Count, ' ', L.Capacity);
  L.TrimExcess;
  WriteLn(L.Count, ' ', L.Capacity);
  L.Add(2);
  WriteLn(L.Count, ' ', L.Capacity);

  WriteLn('Reverse, Move');
  L.AddRange([3, 4, 5]);
  L.Reverse;
  Show('Reverse', L.ToArray);
  L.Move(0, 4);
  Show('Move(0, 4)', L.ToArray);
  L.Move(3, 1);
  Show('Move(3, 1)', L.ToArray);
  L.Free;
end.
