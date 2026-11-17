program DumpGenericKinds;

// The `hir` of the generic types other than classes and records (generics.md 8): a generic
// interface, procedural type and array types are shown with their type parameters, a type
// nested in a generic type with the outer type's name (`TStore<T>.TCell`), and the `class var`s
// of a generic type numbered after the global variables (they are not global variables: each
// instantiation has its own, placed at the end of the global variables, generics.md 2.5). The
// instantiations of the interface, the procedural type and the nested type are listed with
// `.instance`; the `bytecode` has the instantiations' types and `class var`s under their full
// names.

type
  IGetter<T> = interface
    function Get: T;
  end;

  TMap<T, R> = reference to function(const X: T): R;
  TRow<T> = array of T;
  TFour<T> = array[0..3] of T;

  TStore<T> = class(TInterfacedObject, IGetter<T>)
  public
    type
      TCell = record
        Value: T;
      end;
  public
    Cell: TCell;
    class var Count: Integer;
    function Get: T;
  end;

function TStore<T>.Get: T;
begin
  Inc(Count);
  Result := Cell.Value;
end;

var
  G: IGetter<string>;
  M: TMap<Integer, string>;
  R: TRow<Double>;
  F: TFour<Byte>;
begin
  G := TStore<string>.Create;
  M := nil;
  SetLength(R, 1);
  F[0] := 1;
  WriteLn(G.Get = '', Assigned(M), Length(R), F[0], TStore<string>.Count);
end.
