program DumpGenericSharedNames;

// The name of a shared function is the generic form (generics.md 7, 8): the type parameters are
// joined by `,` as in a full name (`TPair<K,V>.Clear`), a generic method keeps its own type
// parameter (`TPair<K,V>.Map<U>`), and a method of a type nested in a generic type has the outer
// type in its name (`TStore<T>.TCell.Reset`). The `; shared by` line lists the instantiations
// with their full names.

type
  TFoo = class end;
  TBar = class end;
  TPair<K, V> = class
    A: K; B: V;
    procedure Clear;
    function Map<U>(const X: U): Integer;
  end;
  TStore<T> = class
  type
    TCell = class
      V: T;
      procedure Reset;
    end;
  end;
procedure TPair<K, V>.Clear; begin A := Default(K); B := Default(V); end;
function TPair<K, V>.Map<U>(const X: U): Integer; begin Result := 1; end;
procedure TStore<T>.TCell.Reset; begin V := Default(T); end;
var P1: TPair<TFoo, Integer>; P2: TPair<TBar, Integer>;
begin
  P1 := TPair<TFoo, Integer>.Create; P1.Clear; WriteLn(P1.Map<TFoo>(nil)); P1.Free;
  P2 := TPair<TBar, Integer>.Create; P2.Clear; WriteLn(P2.Map<TBar>(nil)); P2.Free;
  var C1 := TStore<TFoo>.TCell.Create; C1.Reset; C1.Free;
  var C2 := TStore<TBar>.TCell.Create; C2.Reset; C2.Free;
end.
