program StdlibDictionaryFreedKeys;

// A dictionary whose keys are instances of a class compared by identity keeps working after a key
// is freed while it is in the dictionary (collections.md 7.2, equality.md 2.6): looking it up
// and removing it do not touch the freed instance.

uses System.SysUtils, System.Generics.Collections;

type
  TItem = class
  public
    Name: string;
  end;

var
  D: TDictionary<TItem, Integer>;
  A, B, C: TItem;
  V: Integer;

begin
  A := TItem.Create;
  B := TItem.Create;
  C := TItem.Create;
  D := TDictionary<TItem, Integer>.Create;
  D.Add(A, 1);
  D.Add(B, 2);
  D.Add(C, 3);
  B.Free;
  WriteLn(D.ContainsKey(B), ' ', D.TryGetValue(B, V), ' ', V, ' ', D[A], ' ', D[C]);
  D.Remove(B);
  WriteLn(D.ContainsKey(B), ' ', D.Count);
  // The freed key's handle no longer finds anything once removed; the others still do
  A.Free;
  D[A] := 10;
  WriteLn(D[A], ' ', D.Count);
  D.Remove(A);
  WriteLn(D.Count, ' ', D[C]);
  D.Free;
  C.Free;
end.
