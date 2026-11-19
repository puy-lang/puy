program GenericNestedParamNames;

// A generic type nested in a generic type that names a type parameter of an outer type is
// E0701, as a generic method naming its type's type parameter is (generics.md 2.2, 2.5):
// directly nested, nested through a non-generic type, in a record, and after another type
// parameter. The nested type's own type parameter is still declared and hides the outer one, so
// the uses below and the method's body raise nothing more; other names (`TFine<U>`) are fine. A
// method's type parameter and a member named as the nested type's type parameter point at it.

type
  TA<T> = class
  type
    TIn<T> = class
      F: T;
      procedure Swap;
      procedure Pick<T>;
      procedure T;
    end;
    TMid = class
    type
      TDeep<T> = class
        G: T;
      end;
    end;
    TPair<U, T> = record
      K: U;
      V: T;
    end;
    TFine<U> = class
      H: U;
    end;
  end;

  TR<K> = record
  type
    TCell<K> = record
      C: K;
    end;
  end;

procedure TA<T>.TIn<T>.Swap;
var
  L: T;
begin
  L := F;
  F := L;
end;

var
  X: TA<Integer>.TIn<string>;
  D: TA<Integer>.TMid.TDeep<string>;
  P: TA<Integer>.TPair<Char, string>;
  Y: TA<Integer>.TFine<string>;
  C: TR<Integer>.TCell<string>;
begin
  X := TA<Integer>.TIn<string>.Create;
  X.F := 'a';
  D := TA<Integer>.TMid.TDeep<string>.Create;
  D.G := 'b';
  P.K := 'c';
  P.V := 'd';
  Y := TA<Integer>.TFine<string>.Create;
  Y.H := 'e';
  C.C := 'f';
  WriteLn(X.F, D.G, P.K, P.V, Y.H, C.C);
end.
