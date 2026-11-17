program StdlibOwningDictionaryErrors;

// TObjectDictionary<K, V> (collections.md 9.3, 12.4; generics.md 3.6): a constant Ownerships that
// owns a side whose type argument is not a class is E0768 at the argument, with the type argument
// as secondary and a suggestion removing the element from a set constructor (none for a named
// constant, or for a range of the options); each side gets its own error, and a class side
// passes. A call through an alias has no secondary, even when an argument names another generic
// type. A reference counted class
// for both type arguments is E0760 twice, with the suggestion to use TDictionary on the first
// only.

uses System.Generics.Defaults, System.Generics.Collections;

type
  TFoo = class
  end;

  TRefFoo = class(TRefObject)
  end;

  TPlain = TObjectDictionary<string, string>;
  TByInt = TObjectDictionary<Integer, TFoo>;

const
  OwnKeys = [TDictionaryOwnership.doOwnsKeys];

var
  A: TObjectDictionary<string, TFoo>;
  B: TObjectDictionary<Integer, string>;
  C: TObjectDictionary<TRefFoo, TRefFoo>;
  F: TObjectDictionary<TFoo, TFoo>;
  P: TPlain;
  I: TByInt;
  R: TObjectDictionary<TFoo, string>;

begin
  C := nil;
  A := TObjectDictionary<string, TFoo>.Create([TDictionaryOwnership.doOwnsKeys]);
  A := TObjectDictionary<string, TFoo>.Create([TDictionaryOwnership.doOwnsValues,
    TDictionaryOwnership.doOwnsKeys]);
  A := TObjectDictionary<string, TFoo>.Create(OwnKeys, 16);
  B := TObjectDictionary<Integer, string>.Create([TDictionaryOwnership.doOwnsKeys,
    TDictionaryOwnership.doOwnsValues]);
  P := TPlain.Create([TDictionaryOwnership.doOwnsValues]);
  F := TObjectDictionary<TFoo, TFoo>.Create(OwnKeys + [TDictionaryOwnership.doOwnsValues]);
  A := TObjectDictionary<string, TFoo>.Create([TDictionaryOwnership.doOwnsValues]);
  I := TByInt.Create([TDictionaryOwnership.doOwnsKeys], TEqualityComparer<Integer>.Default);
  R := TObjectDictionary<TFoo, string>.Create([TDictionaryOwnership.doOwnsKeys..
    TDictionaryOwnership.doOwnsValues]);
end.
