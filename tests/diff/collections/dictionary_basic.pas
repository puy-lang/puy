program DictionaryBasic;

// TDictionary<string, Integer> (collections.md 7.1): adding, looking up and removing keys. The
// output does not depend on the order of the entries, which FPC does not keep.

uses System.SysUtils, System.Generics.Collections;

var
  D: TDictionary<string, Integer>;
  V: Integer;

procedure Show(const Key: string);
begin
  if D.TryGetValue(Key, V) then
    WriteLn(Key, ' -> ', V)
  else
    WriteLn(Key, ' missing');
end;

begin
  D := TDictionary<string, Integer>.Create;
  try
    D.Add('one', 1);
    D.Add('two', 2);
    D.Add('three', 3);
    WriteLn('count ', D.Count);
    Show('one');
    Show('two');
    Show('four');
    WriteLn('contains two: ', D.ContainsKey('two'));
    WriteLn('contains TWO: ', D.ContainsKey('TWO'));
    WriteLn('items[three] = ', D['three']);

    D.Remove('two');
    D.Remove('nothing');
    WriteLn('count ', D.Count);
    WriteLn('contains two: ', D.ContainsKey('two'));
    Show('two');

    D.AddOrSetValue('one', 10);
    D.AddOrSetValue('five', 5);
    D['three'] := 30;
    WriteLn('count ', D.Count);
    Show('one');
    Show('three');
    Show('five');

    D.Add('two', 20);
    Show('two');
    WriteLn('contains value 20: ', D.ContainsValue(20));
    WriteLn('contains value 2: ', D.ContainsValue(2));

    D.Clear;
    WriteLn('count ', D.Count);
    Show('one');
  finally
    D.Free;
  end;
end.
