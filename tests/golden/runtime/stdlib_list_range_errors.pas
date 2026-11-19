program StdlibListRangeErrors;

// The out-of-range errors of TList<T> (collections.md 3.2): ERangeError with the index and the
// valid range, `0..Count` for Insert and InsertRange (also of no items), the whole range for
// DeleteRange (none for no items), no index for First / Last of an empty list. A member that
// raises leaves the list as it was.

uses System.SysUtils, System.Generics.Collections;

var
  L: TList<Integer>;

procedure Show;
var
  X: Integer;
begin
  Write('  list:');
  for X in L do
    Write(' ', X);
  WriteLn(' (count ', L.Count, ', capacity ', L.Capacity, ')');
end;

procedure Report(const E: Exception);
begin
  WriteLn(E.ClassName, ': ', E.Message);
end;

begin
  L := TList<Integer>.Create;

  WriteLn('empty');
  try WriteLn(L[0]); except on E: Exception do Report(E); end;
  try WriteLn(L.First); except on E: Exception do Report(E); end;
  try WriteLn(L.Last); except on E: Exception do Report(E); end;
  try L.Delete(0); except on E: Exception do Report(E); end;
  try L.Insert(1, 9); except on E: Exception do Report(E); end;
  Show;

  L.AddRange([10, 20, 30]);
  WriteLn('three items');
  try WriteLn(L[3]); except on E: Exception do Report(E); end;
  try WriteLn(L[-1]); except on E: Exception do Report(E); end;
  try L[5] := 1; except on E: Exception do Report(E); end;
  try L.Insert(4, 9); except on E: Exception do Report(E); end;
  try L.Insert(-1, 9); except on E: Exception do Report(E); end;
  try L.InsertRange(5, [1, 2]); except on E: Exception do Report(E); end;
  // Inserting no items still checks the index, unlike DeleteRange of no items below.
  try L.InsertRange(9, []); except on E: Exception do Report(E); end;
  L.InsertRange(3, []);
  try L.Delete(3); except on E: Exception do Report(E); end;
  try L.ExtractAt(7); except on E: Exception do Report(E); end;
  try L.Exchange(0, 3); except on E: Exception do Report(E); end;
  try L.Move(-2, 0); except on E: Exception do Report(E); end;
  Show;

  WriteLn('DeleteRange');
  L.AddRange([40, 50]);
  try L.DeleteRange(3, 5); except on E: Exception do Report(E); end;
  try L.DeleteRange(-1, 2); except on E: Exception do Report(E); end;
  try L.DeleteRange(0, -1); except on E: Exception do Report(E); end;
  try L.DeleteRange(5, 1); except on E: Exception do Report(E); end;
  // The last index of the range is past MaxInt; the message must not overflow.
  try L.DeleteRange(MaxInt, 2); except on E: Exception do Report(E); end;
  // Removing nothing never fails.
  L.DeleteRange(9, 0);
  Show;
  L.DeleteRange(3, 2);
  Show;

  WriteLn('Count and Capacity');
  try L.Count := -1; except on E: Exception do Report(E); end;
  try L.Capacity := 2; except on E: Exception do Report(E); end;
  Show;
  L.Capacity := 3;
  Show;
  L.Free;
end.
