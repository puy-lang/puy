program StdlibDictionaryComparer;

// A dictionary given an equality comparer (collections.md 7.4) compares and hashes its keys with
// it: TStringComparer.OrdinalIgnoreCase makes the keys case-insensitive in ASCII, keeping the key
// as first added. TEqualityComparer<K>.Default and Construct work the same way.

uses System.SysUtils, System.Generics.Defaults, System.Generics.Collections;

var
  D: TDictionary<string, Integer>;
  N: TDictionary<Integer, string>;
  P: TPair<string, Integer>;
  V: Integer;

begin
  D := TDictionary<string, Integer>.Create(TStringComparer.OrdinalIgnoreCase);
  D.Add('Hello', 1);
  D.Add('World', 2);
  WriteLn(D.ContainsKey('HELLO'), ' ', D.ContainsKey('world'), ' ', D.ContainsKey('Hellö'));
  D['hello'] := 10;
  D.AddOrSetValue('WORLD', 20);
  try
    D.Add('hELLO', 3);
  except
    on E: EListError do WriteLn(E.Message);
  end;
  for P in D do
    WriteLn(P.Key, '=', P.Value);
  D.Remove('HeLLo');
  WriteLn(D.Count, ' ', D.TryGetValue('hello', V), ' ', V);
  D.Free;

  D := TDictionary<string, Integer>.Create(4, TEqualityComparer<string>.Default);
  D.Add('a', 1);
  WriteLn(D.ContainsKey('a'), ' ', D.ContainsKey('A'), ' ', D.Capacity);
  D.Free;

  // Keys equal when they have the same remainder by 10
  N := TDictionary<Integer, string>.Create(TEqualityComparer<Integer>.Construct(
    function(const Left, Right: Integer): Boolean
    begin
      Result := Left mod 10 = Right mod 10;
    end,
    function(const Value: Integer): Int64
    begin
      Result := Value mod 10;
    end));
  N.Add(1, 'one');
  N.Add(12, 'twelve');
  N[21] := 'twenty-one';
  WriteLn(N.Count, ' ', N[11], ' ', N[2], ' ', N.ContainsKey(3));
  N.Free;
end.
