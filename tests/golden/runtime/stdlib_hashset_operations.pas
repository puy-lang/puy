program StdlibHashSetOperations;

// UnionWith, IntersectWith and ExceptWith write the union, intersection and difference into the
// set (collections.md 8): the items that stay keep their insertion order, and the union adds the
// new items at the end in the order of the source. Each takes another container or an array.

uses System.SysUtils, System.Generics.Collections;

var
  S, Other: THashSet<Integer>;
  L: TList<Integer>;

procedure Show(const Title: string; ASet: THashSet<Integer>);
var
  Y: Integer;
begin
  Write(Title, ' (', ASet.Count, '):');
  for Y in ASet do
    Write(' ', Y);
  WriteLn;
end;

begin
  S := THashSet<Integer>.Create([5, 3, 8, 1]);
  Show('start', S);
  S.UnionWith([9, 3, 2, 9, 7]);
  Show('union with 9 3 2 9 7', S);
  S.IntersectWith([7, 1, 4, 5, 2, 1]);
  Show('intersect with 7 1 4 5 2 1', S);
  S.ExceptWith([2, 6, 5]);
  Show('except 2 6 5', S);

  Other := THashSet<Integer>.Create([30, 1, 20, 10]);
  S.UnionWith(Other);
  Show('union with a set', S);
  L := TList<Integer>.Create;
  L.AddRange([10, 7, 30, 40]);
  S.IntersectWith(L);
  Show('intersect with a list', S);
  L.Clear;
  L.Add(30);
  S.ExceptWith(L);
  Show('except a list', S);

  // The set itself as the source
  S.UnionWith(S);
  Show('union with itself', S);
  S.IntersectWith(S);
  Show('intersect with itself', S);
  S.ExceptWith(S);
  Show('except itself', S);

  // Empty sources
  S.UnionWith([4, 6]);
  S.UnionWith([]);
  Show('union with nothing', S);
  S.ExceptWith([]);
  Show('except nothing', S);
  S.IntersectWith([]);
  Show('intersect with nothing', S);

  L.Free;
  Other.Free;
  S.Free;
end.
