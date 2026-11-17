program StdlibBinarySearch;

// BinarySearch (collections.md 10.3): True with the smallest index of the equal items, False
// with the index the item would be inserted at to keep the order (0 .. Count), for TList<T>
// with the default order and with comparers, and for TArray.BinarySearch.

uses System.SysUtils, System.Generics.Defaults, System.Generics.Collections;

function Descending(const A, B: Integer): Integer;
begin
  Result := B - A;
end;

var
  L: TList<Integer>;
  A, Words: TArray<string>;
  Keys: TArray<Integer>;
  X, Found: Integer;
  S: string;
begin
  L := TList<Integer>.Create;
  L.AddRange([1, 3, 3, 3, 5, 5, 7, 9, 9, 9, 9, 11]);
  Keys := [0, 1, 3, 4, 5, 9, 10, 11, 12];
  for X in Keys do
  begin
    Found := -1;
    WriteLn('search ', X, ': ', L.BinarySearch(X, Found), ' at ', Found);
  end;
  L.Insert(4, 4);
  WriteLn('after Insert(4, 4): ', L.BinarySearch(4, Found), ' at ', Found);
  L.Free;

  L := TList<Integer>.Create;
  WriteLn('empty list: ', L.BinarySearch(5, Found), ' at ', Found);
  L.Free;

  L := TList<Integer>.Create(TComparer<Integer>.Construct(Descending));
  L.AddRange([8, 6, 6, 4, 2]);
  WriteLn('descending (Create): ', L.BinarySearch(6, Found), ' at ', Found);
  WriteLn('descending (Create): ', L.BinarySearch(5, Found), ' at ', Found);
  L.Sort(nil);
  // nil is the default order, not the comparer given to Create
  WriteLn('Sort(nil), BinarySearch(nil): ', L.BinarySearch(6, Found, nil), ' at ', Found);
  L.Free;

  A := ['ant', 'bee', 'bee', 'cat', 'dog'];
  Words := ['bee', 'ant', 'cow', 'eel', ''];
  for S in Words do
    WriteLn('TArray ', S, ': ', TArray.BinarySearch<string>(A, S, Found), ' at ', Found);
end.
