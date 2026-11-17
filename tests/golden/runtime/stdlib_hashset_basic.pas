program StdlibHashSetBasic;

// THashSet<T> (collections.md 8): Add and Remove tell whether they changed the set, Contains
// whether the item is there, and the items are kept in insertion order. Creating from an array or
// another container silently keeps one of each repeated item, the first.

uses System.SysUtils, System.Generics.Collections;

var
  S, Other: THashSet<string>;
  L: TList<string>;
  E: TEnumerable<string>;
  Items: TArray<string>;
  X: string;

procedure Show(const Title: string; ASet: THashSet<string>);
var
  Y: string;
begin
  Write(Title, ' (', ASet.Count, '):');
  for Y in ASet do
    Write(' ', Y);
  WriteLn;
end;

begin
  S := THashSet<string>.Create;
  WriteLn('add b: ', S.Add('b'));
  WriteLn('add a: ', S.Add('a'));
  WriteLn('add b again: ', S.Add('b'));
  WriteLn('add c: ', S.Add('c'));
  Show('after adding', S);
  WriteLn('contains a: ', S.Contains('a'), ', contains z: ', S.Contains('z'));
  WriteLn('remove z: ', S.Remove('z'));
  WriteLn('remove b: ', S.Remove('b'));
  WriteLn('remove b again: ', S.Remove('b'));
  WriteLn('contains b: ', S.Contains('b'));
  Show('after removing', S);
  // An item removed and added again goes to the end
  S.Add('b');
  Show('b added again', S);
  S.Clear;
  WriteLn('remove from empty: ', S.Remove('a'));
  Show('cleared', S);
  S.Free;

  S := THashSet<string>.Create(['q', 'p', 'q', 'r', 'p', 'q']);
  Show('from an array', S);

  L := TList<string>.Create;
  L.AddRange(['y', 'x', 'y', 'z', 'x']);
  Other := THashSet<string>.Create(L);
  Show('from a list', Other);
  L.Free;

  // ToArray and enumerating a value whose static type is TEnumerable<T> keep the order
  Items := S.ToArray;
  WriteLn('ToArray: ', Length(Items), ' ', Items[0], Items[1], Items[2]);
  E := Other;
  Write('as TEnumerable:');
  for X in E do
    Write(' ', X);
  WriteLn;

  // Capacity grows by doubling; TrimExcess cuts it down to the count
  WriteLn('capacity ', S.Capacity);
  S.Add('s');
  S.Add('t');
  WriteLn('capacity ', S.Capacity);
  S.TrimExcess;
  WriteLn('capacity ', S.Capacity, ' after TrimExcess');
  S.Capacity := 10;
  Show('capacity 10', S);
  S.Free;
  Other.Free;
end.
