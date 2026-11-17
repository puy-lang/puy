program StdlibListModified;

// Changing the structure of a TList<T> while a for..in walks it raises EInvalidOperation at the
// next MoveNext (collections.md 3.3), also when the change is made in the last iteration.
// Writing a value in place (L[I] := X) is not a change of the structure. Walking a copy made by
// ToArray, or a list seen as TEnumerable<T>, does not see the change. Members that add or
// remove no item (Remove of a missing value, an empty range) change nothing.

uses System.SysUtils, System.Generics.Collections;

var
  L: TList<Integer>;
  E: TEnumerable<Integer>;
  X: Integer;

procedure Reset;
begin
  L.Clear;
  L.AddRange([1, 2, 3]);
end;

procedure Report(const E: Exception);
begin
  WriteLn('  ', E.ClassName, ': ', E.Message);
end;

begin
  L := TList<Integer>.Create;

  WriteLn('Add in the first iteration');
  Reset;
  try
    for X in L do
    begin
      WriteLn('  visit ', X);
      L.Add(4);
    end;
  except
    on E: Exception do Report(E);
  end;

  WriteLn('Delete in the last iteration');
  Reset;
  try
    for X in L do
    begin
      WriteLn('  visit ', X);
      if X = 3 then
        L.Delete(0);
    end;
    WriteLn('  not raised');
  except
    on E: Exception do Report(E);
  end;

  WriteLn('each change of the structure');
  Reset;
  WriteLn('  Insert');
  try for X in L do L.Insert(0, 0); except on E: Exception do Report(E); end;
  Reset;
  WriteLn('  Exchange');
  try for X in L do L.Exchange(0, 1); except on E: Exception do Report(E); end;
  Reset;
  WriteLn('  Move');
  try for X in L do L.Move(0, 1); except on E: Exception do Report(E); end;
  Reset;
  WriteLn('  Reverse');
  try for X in L do L.Reverse; except on E: Exception do Report(E); end;
  Reset;
  WriteLn('  Count');
  try for X in L do L.Count := 3; except on E: Exception do Report(E); end;
  Reset;
  WriteLn('  Capacity');
  try for X in L do L.Capacity := 10; except on E: Exception do Report(E); end;
  Reset;
  WriteLn('  TrimExcess');
  try for X in L do L.TrimExcess; except on E: Exception do Report(E); end;
  Reset;
  WriteLn('  Clear');
  try for X in L do L.Clear; except on E: Exception do Report(E); end;

  // Remove that finds nothing and a range of no items change nothing, so they are not changes.
  WriteLn('adding or removing nothing');
  Reset;
  try for X in L do L.Remove(9); except on E: Exception do Report(E); end;
  Reset;
  try for X in L do L.AddRange([]); except on E: Exception do Report(E); end;
  Reset;
  try for X in L do L.DeleteRange(0, 0); except on E: Exception do Report(E); end;
  WriteLn('  not raised');

  WriteLn('writing values in place');
  Reset;
  for X in L do
  begin
    WriteLn('  visit ', X);
    L[2] := L[2] * 10;
  end;

  WriteLn('a copy');
  Reset;
  for X in L.ToArray do
  begin
    WriteLn('  visit ', X);
    L.Add(X + 10);
  end;
  E := L;
  for X in E do
  begin
    WriteLn('  visit ', X);
    L.Delete(0);
  end;
  WriteLn('  count ', L.Count);
  L.Free;
end.
