program StdlibSortModified;

// A comparer that changes the list during Sort (collections.md 10.3): EInvalidOperation at the
// end of the sort, with the text of a change during enumeration (3.3); the list keeps what the
// comparer made of it, unsorted. An enumeration begun before the sort sees the sort as a change.
// BinarySearch with such a comparer raises nothing and answers an index within the items it was
// called with (5.6).

uses System.SysUtils, System.Generics.Defaults, System.Generics.Collections;

var
  L: TList<Integer>;
  Added: Boolean;

function AddingOnce(const A, B: Integer): Integer;
begin
  if not Added then
  begin
    Added := True;
    L.Add(100);
  end;
  Result := A - B;
end;

function Writing(const A, B: Integer): Integer;
begin
  L[0] := 50;
  L.Add(60);
  Result := A - B;
end;

function Clearing(const A, B: Integer): Integer;
begin
  L.Clear;
  Result := A - B;
end;

procedure Show;
var
  X: Integer;
begin
  Write('  list:');
  for X in L do
    Write(' ', X);
  WriteLn(' (count ', L.Count, ')');
end;

var
  X, Found: Integer;
begin
  L := TList<Integer>.Create;
  L.AddRange([5, 3, 9, 1, 7]);
  Added := False;
  try
    L.Sort(TComparer<Integer>.Construct(AddingOnce));
  except
    on E: EInvalidOperation do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  Show;

  try
    L.Sort(TComparer<Integer>.Construct(Clearing));
  except
    on E: EInvalidOperation do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  Show;

  try
    for X in L do
      L.Sort;
  except
    on E: EInvalidOperation do
      WriteLn('Sort in for..in: ', E.Message);
  end;
  WriteLn('BinarySearch with a comparer that changes the list:');
  L.AddRange([1, 3, 5, 7, 9]);
  WriteLn('  ', L.BinarySearch(7, Found, TComparer<Integer>.Construct(Writing)), ' at ', Found);
  Show;
  L.Free;
end.
