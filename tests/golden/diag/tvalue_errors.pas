program TValueErrors;

// Diagnostics of TValue and `array of const` (rtti.md 3.8, 4.1, chapter 9): TValue has no `=`
// (E0703) and no key equality (E0778, at the type arguments of an instantiation, or at
// KeyEquals / KeyHash, also of a record holding one), and a routine name is not an element of an
// `array of const` (E0777)

uses System.Generics.Collections;

type
  TRec = record
    V: TValue;
  end;

procedure P;
begin
end;

procedure Show(const Args: array of const);
begin
end;

var
  Dict: TDictionary<TValue, Integer>;
  X, Y: TValue;
  R: TRec;
begin
  Dict := TDictionary<TValue, Integer>.Create;
  Dict.Free;
  WriteLn(KeyEquals(X, Y));
  WriteLn(KeyHash(R));
  Show([P]);
  WriteLn(X = Y);
end.
