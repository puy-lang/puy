program GenericSelfContainingRecord;

// A record or static array that contains itself through the fields of an instantiation of a
// generic type (types.md 1.1, generics.md 4.2) is E0727, as a direct cycle is: a generic record
// with the record as its type argument, a generic static array, a type argument after another,
// and a generic record whose field instantiates a generic type with the record itself. An
// instantiation of a generic type reported here (`TSelf<Integer>`) is not reported again, and
// using the types does not make the compiler overflow its stack. Two cycles of records that are
// not instantiations are both reported (`TA` and `TB`), as without generics.

type
  TFoo<T> = record
    V: T;
  end;
  TBox<T> = array[0..3] of T;
  TPair<K, V> = record
    Key: K;
    Value: V;
  end;

  TBar = record
    A: TFoo<TBar>;
    N: Integer;
  end;
  TR = record
    A: TBox<TR>;
  end;
  TQ = record
    P: TPair<Integer, TQ>;
  end;
  TSelf<T> = record
    B: TBox<TSelf<T>>;
  end;
  TUse = record
    S: TSelf<Integer>;
  end;
  TA = record
    X: TA;
    Y: TB;
  end;
  TB = record
    Z: TA;
  end;

procedure Go;
var
  x, y: TBar;
  u: TUse;
begin
  x := Default(TBar);
  y := x;
  WriteLn(y.N);
  u := Default(TUse);
end;

begin
  Go;
end.
