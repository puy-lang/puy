program ListSort;

// TList<T>.Sort with the default order (collections.md 5.1, 10.2, 10.3). No two items are equal,
// so the result does not depend on the stability of the sort, which differs from FPC. Comparers
// (`constref` in FPC), BinarySearch (an `out` SizeInt in FPC) and TArray (FPC 3.2.2 does not take
// `TArray.Sort<Integer>(A)`) are in golden/runtime.

uses System.SysUtils, System.Generics.Collections;

procedure ShowInts(const L: TList<Integer>);
var
  I: Integer;
begin
  for I := 0 to L.Count - 1 do
    Write(L[I], ' ');
  WriteLn;
end;

var
  L: TList<Integer>;
  S: TList<string>;
  I: Integer;
begin
  L := TList<Integer>.Create;
  for I := 1 to 30 do
    L.Add((I * 17) mod 31);
  L.Sort;
  ShowInts(L);
  L.Clear;
  L.AddRange([4, -2, 9, 0]);
  L.Sort;
  ShowInts(L);
  L.Free;

  S := TList<string>.Create;
  S.AddRange(['pear', 'apple', 'fig', 'kiwi', 'banana', 'cherry']);
  S.Sort;
  for I := 0 to S.Count - 1 do
    Write(S[I], ' ');
  WriteLn;
  S.Free;
end.
