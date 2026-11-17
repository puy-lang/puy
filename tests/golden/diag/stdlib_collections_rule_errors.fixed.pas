program StdlibCollectionsRuleErrors;

// Diagnostics of the containers not seen elsewhere (collections.md 3.1, 5.4, 9.5, 12.6): a field
// of a record item cannot be written through Items[I] (E0707); a slice cannot be assigned to;
// TObjectDictionary<K, V>.Create([]) fits both the empty set and an empty array of pairs (E0734);
// a subclass that inherits Create is checked like TObjectDictionary itself (E0768); there is no
// change notification (OnNotify is E0700); EListError without `uses System.SysUtils` is E0700
// with a fix that adds it (collections.md 1.2).

uses System.Generics.Collections, System.SysUtils;

type
  TPoint = record
    X, Y: Integer;
  end;

  TFoo = class
  end;

  TOwning = class(TObjectDictionary<string, TFoo>)
  end;

var
  P: TList<TPoint>;
  L: TList<Integer>;
  D: TObjectDictionary<TFoo, TFoo>;
  O: TOwning;
begin
  P := TList<TPoint>.Create;
  P[0].X := 1;
  L := TList<Integer>.Create;
  L[0..1] := [1, 2];
  D := TObjectDictionary<TFoo, TFoo>.Create([]);
  O := TOwning.Create([TDictionaryOwnership.doOwnsKeys]);
  L.OnNotify := nil;
  try
    L.Delete(0);
  except
    on E: EListError do
      WriteLn(E.Message);
  end;
end.
