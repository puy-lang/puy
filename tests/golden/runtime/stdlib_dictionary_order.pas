program StdlibDictionaryOrder;

// A dictionary keeps its entries in insertion order (collections.md 7.3): a key removed and added
// again goes to the end, and writing the value of a key already there (AddOrSetValue,
// Items[K] := V) leaves it where it was. The order holds through growing and through squeezing
// out the holes that removing leaves.

uses System.SysUtils, System.Generics.Collections;

var
  D: TDictionary<string, Integer>;
  P: TPair<string, Integer>;
  A: TArray<TPair<string, Integer>>;
  I: Integer;

procedure Show(const Title: string);
var
  Pair: TPair<string, Integer>;
begin
  Write(Title, ':');
  for Pair in D do
    Write(' ', Pair.Key, '=', Pair.Value);
  WriteLn;
end;

begin
  D := TDictionary<string, Integer>.Create;
  D.Add('c', 1);
  D.Add('a', 2);
  D.Add('b', 3);
  Show('added');

  D.Remove('c');
  D.Add('c', 4);
  Show('c removed and added');

  D.AddOrSetValue('a', 5);
  D['b'] := 6;
  D.AddOrSetValue('d', 7);
  D['e'] := 8;
  Show('values written');

  D.TryAdd('a', 9);
  D.TryAdd('f', 10);
  Show('TryAdd');

  P := D.ExtractPair('a');
  WriteLn('extracted ', P.Key, '=', P.Value);
  P := D.ExtractPair('zz');
  WriteLn('not there ', P.Key, '=', P.Value);
  Show('after ExtractPair');

  A := D.ToArray;
  Write('ToArray:');
  for I := 0 to High(A) do
    Write(' ', A[I].Key);
  WriteLn;

  // Many removals squeeze the holes out; growing places the entries again
  D.Clear;
  for I := 1 to 100 do
    D.Add('k' + IntToStr(I), I);
  for I := 1 to 100 do
    if I mod 10 <> 0 then
      D.Remove('k' + IntToStr(I));
  for I := 101 to 103 do
    D.Add('k' + IntToStr(I), I);
  D.Add('k1', 1);
  Show('after squeezing');
  WriteLn('count ', D.Count);
  D.TrimExcess;
  WriteLn('capacity after TrimExcess ', D.Capacity);
  Show('after TrimExcess');
  D.Capacity := 40;
  WriteLn('capacity ', D.Capacity);
  Show('after Capacity');
  D.Free;
end.
