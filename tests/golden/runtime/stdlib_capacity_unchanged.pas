program StdlibCapacityUnchanged;

// Writing the capacity a container already has, and TrimExcess when the capacity is already the
// count, are not changes of the structure (collections.md 3.3): a for..in goes on. Writing
// another capacity and TrimExcess that frees slots are changes, the same for TList<T>,
// TStack<T>, TQueue<T>, TDictionary<K, V> and THashSet<T>.

uses System.SysUtils, System.Generics.Collections;

procedure Report(const What: string; E: Exception);
begin
  WriteLn('  ', What, ': ', E.ClassName, ': ', E.Message);
end;

var
  L: TList<Integer>;
  S: TStack<Integer>;
  Q: TQueue<Integer>;
  D: TDictionary<Integer, Integer>;
  H: THashSet<Integer>;
  X, N: Integer;
  P: TPair<Integer, Integer>;
begin
  WriteLn('TList');
  L := TList<Integer>.Create([1, 2, 3]);
  L.Capacity := 5;
  N := 0;
  for X in L do
  begin
    L.Capacity := 5;
    Inc(N);
  end;
  WriteLn('  same capacity: ', N, ' visits');
  try for X in L do L.TrimExcess; except on E: Exception do Report('TrimExcess', E); end;
  WriteLn('  capacity ', L.Capacity);
  N := 0;
  for X in L do
  begin
    L.TrimExcess;
    Inc(N);
  end;
  WriteLn('  TrimExcess at the count: ', N, ' visits');
  try for X in L do L.Capacity := 4; except on E: Exception do Report('Capacity', E); end;
  L.Free;

  WriteLn('TStack');
  S := TStack<Integer>.Create([1, 2, 3]);
  N := 0;
  for X in S do
  begin
    S.TrimExcess;
    S.Capacity := 3;
    Inc(N);
  end;
  WriteLn('  unchanged: ', N, ' visits');
  try for X in S do S.Capacity := 4; except on E: Exception do Report('Capacity', E); end;
  try for X in S do S.TrimExcess; except on E: Exception do Report('TrimExcess', E); end;
  S.Free;

  WriteLn('TQueue');
  Q := TQueue<Integer>.Create([1, 2, 3]);
  N := 0;
  for X in Q do
  begin
    Q.TrimExcess;
    Q.Capacity := 3;
    Inc(N);
  end;
  WriteLn('  unchanged: ', N, ' visits');
  try for X in Q do Q.Capacity := 4; except on E: Exception do Report('Capacity', E); end;
  try for X in Q do Q.TrimExcess; except on E: Exception do Report('TrimExcess', E); end;
  Q.Free;

  WriteLn('TDictionary');
  D := TDictionary<Integer, Integer>.Create(3);
  D.Add(1, 10);
  D.Add(2, 20);
  D.Add(3, 30);
  N := 0;
  for P in D do
  begin
    D.TrimExcess;
    D.Capacity := 3;
    Inc(N);
  end;
  WriteLn('  unchanged: ', N, ' visits');
  try for P in D do D.Capacity := 4; except on E: Exception do Report('Capacity', E); end;
  try for P in D do D.TrimExcess; except on E: Exception do Report('TrimExcess', E); end;
  D.Free;

  WriteLn('THashSet');
  H := THashSet<Integer>.Create([1, 2, 3]);
  WriteLn('  capacity ', H.Capacity);
  H.TrimExcess;
  N := 0;
  for X in H do
  begin
    H.TrimExcess;
    H.Capacity := 3;
    Inc(N);
  end;
  WriteLn('  unchanged: ', N, ' visits');
  try for X in H do H.Capacity := 4; except on E: Exception do Report('Capacity', E); end;
  try for X in H do H.TrimExcess; except on E: Exception do Report('TrimExcess', E); end;
  H.Free;
end.
