program StdlibCollectionsCreateErrors;

// A literal nil given to Create of a container fits the versions taking a comparer, an array and
// a TEnumerable<T> alike, so the call is ambiguous (E0734; collections.md 5.6), like Create([]) of
// TObjectDictionary<K, V> (9.5). A stack or a queue has no comparer, but nil still fits the array
// and the TEnumerable<T>. Create without arguments, or a nil comparer held in a typed variable,
// picks one version and passes. Only the dictionary makes its Keys and Values views: their
// constructors are private (E0743; 7.3).

uses System.Generics.Defaults, System.Generics.Collections;

type
  TFoo = class
  end;

var
  L: TList<Integer>;
  O: TObjectList<TFoo>;
  D: TDictionary<string, Integer>;
  H: THashSet<Integer>;
  S: TStack<Integer>;
  Q: TQueue<Integer>;
  C: IComparer<Integer>;
  E: IEqualityComparer<string>;
begin
  L := TList<Integer>.Create(nil);
  O := TObjectList<TFoo>.Create(nil);
  D := TDictionary<string, Integer>.Create(nil);
  H := THashSet<Integer>.Create(nil);
  S := TStack<Integer>.Create(nil);
  Q := TQueue<Integer>.Create(nil);

  C := nil;
  E := nil;
  L := TList<Integer>.Create(C);
  D := TDictionary<string, Integer>.Create(E);
  H := THashSet<Integer>.Create;

  WriteLn(TDictionary<string, Integer>.TKeyCollection.Create(D).Count);
  WriteLn(TDictionary<string, Integer>.TValueCollection.Create(D).Count);
  WriteLn(D.Keys.Count, D.Values.Count);
end.
