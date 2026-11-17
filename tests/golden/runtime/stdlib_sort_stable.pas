program StdlibSortStable;

// The sort is stable (collections.md 10.3): items the order calls equal keep the order they had,
// with the comparer given to Create, to Sort, and to TArray.Sort, also past the length sorted by
// insertion (8) where the runs are merged, and on items already in order. TArray.Sort with
// Index / Count sorts only that range.

uses System.SysUtils, System.Generics.Defaults, System.Generics.Collections;

type
  TItem = record
    Key: Integer;
    Tag: string;
  end;

function Item(Key: Integer; const Tag: string): TItem;
begin
  Result := Default(TItem);
  Result.Key := Key;
  Result.Tag := Tag;
end;

function ByKey(const A, B: TItem): Integer;
begin
  Result := A.Key - B.Key;
end;

function ByKeyDown(const A, B: TItem): Integer;
begin
  Result := B.Key - A.Key;
end;

procedure Show(const L: TList<TItem>);
var
  X: TItem;
begin
  for X in L do
    Write(X.Key, X.Tag, ' ');
  WriteLn;
end;

procedure ShowArray(const A: array of TItem);
var
  X: TItem;
begin
  for X in A do
    Write(X.Key, X.Tag, ' ');
  WriteLn;
end;

var
  L: TList<TItem>;
  A: TArray<TItem>;
  I: Integer;
begin
  L := TList<TItem>.Create(TComparer<TItem>.Construct(ByKey));
  for I := 0 to 24 do
    L.Add(Item(I mod 4, Chr(Ord('a') + I)));
  WriteLn('Create(AComparer), 25 items:');
  L.Sort;
  Show(L);
  WriteLn('Sort(AComparer), descending:');
  L.Sort(TComparer<TItem>.Construct(ByKeyDown));
  Show(L);
  WriteLn('already in order, 20 items (the merges copy the runs):');
  L.Clear;
  for I := 0 to 19 do
    L.Add(Item(I div 3, Chr(Ord('a') + I)));
  L.Sort;
  Show(L);
  L.Free;

  SetLength(A, 12);
  for I := 0 to 11 do
    A[I] := Item(I mod 3, Chr(Ord('a') + I));
  WriteLn('TArray.Sort, 12 items:');
  TArray.Sort<TItem>(A, TComparer<TItem>.Construct(ByKey));
  ShowArray(A);
  for I := 0 to 11 do
    A[I] := Item(I mod 3, Chr(Ord('a') + I));
  WriteLn('TArray.Sort of the range 2..9:');
  TArray.Sort<TItem>(A, TComparer<TItem>.Construct(ByKey), 2, 8);
  ShowArray(A);
end.
