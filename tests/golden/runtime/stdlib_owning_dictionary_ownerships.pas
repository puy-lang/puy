program StdlibOwningDictionaryOwnerships;

// Owning a side of TObjectDictionary<K, V> whose type is not a class, with Ownerships that is not
// a constant expression, raises EInvalidOperation when the dictionary is created
// (collections.md 9.3): no instance is made (the variable keeps its value). The keys are checked
// first. A class type for the owned side passes. A constant written in a generic body for a
// side whose type is a type parameter is not E0768 (the compiler cannot tell from the body): an
// instantiation with a class passes, and one with another type raises when it creates.

uses System.SysUtils, System.Generics.Collections;

type
  TItem = class
  end;

  TMaker<T: NotRefCounted> = class
    class function Make: TObjectDictionary<T, TItem>;
  end;

class function TMaker<T>.Make: TObjectDictionary<T, TItem>;
begin
  Result := TObjectDictionary<T, TItem>.Create([TDictionaryOwnership.doOwnsKeys]);
end;

procedure Report(E: Exception);
begin
  WriteLn(E.ClassName, ': ', E.Message);
end;

var
  Own: TDictionaryOwnerships;
  A: TObjectDictionary<string, TItem>;
  B: TObjectDictionary<Integer, Double>;
  C: TObjectDictionary<TItem, TItem>;
  M: TObjectDictionary<Integer, TItem>;
  N: TObjectDictionary<TItem, TItem>;

begin
  Own := [TDictionaryOwnership.doOwnsKeys];
  A := nil;
  try
    A := TObjectDictionary<string, TItem>.Create(Own);
  except
    on E: EInvalidOperation do
      Report(E);
  end;
  WriteLn('assigned ', Assigned(A));

  Include(Own, TDictionaryOwnership.doOwnsValues);
  try
    B := TObjectDictionary<Integer, Double>.Create(Own, 8);
    B.Free;
  except
    on E: EInvalidOperation do
      Report(E);
  end;
  Exclude(Own, TDictionaryOwnership.doOwnsKeys);
  try
    B := TObjectDictionary<Integer, Double>.Create(Own);
    B.Free;
  except
    on E: EInvalidOperation do
      Report(E);
  end;
  A := TObjectDictionary<string, TItem>.Create(Own);
  WriteLn('owns values ', TDictionaryOwnership.doOwnsValues in A.Ownerships);
  A.Free;

  Include(Own, TDictionaryOwnership.doOwnsKeys);
  C := TObjectDictionary<TItem, TItem>.Create(Own);
  WriteLn('owns both ', C.Ownerships = Own);
  C.Free;

  N := TMaker<TItem>.Make;
  WriteLn('generic class ', N.Count);
  N.Free;
  try
    M := TMaker<Integer>.Make;
    M.Free;
  except
    on E: EInvalidOperation do
      Report(E);
  end;
end.
