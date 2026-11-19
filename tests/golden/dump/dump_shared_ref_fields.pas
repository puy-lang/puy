program DumpSharedRefFields;

// The type word of `RefFieldSet` (bytecode.md 1.2, 2.8) gives only a shape to the VM, so the
// functions of instantiations whose records have the same shape are shared (generics.md 7):
// `TBox<T>.Bump` for two classes and `Put<T>` for `Integer` and `Int64`, keeping the type word of
// the first. `Put<string>` writes a managed field and is not shared with them.

type
  TA = class end;
  TB = class end;
  TBox<T: class> = record
    Item: T;
    N: Integer;
    procedure Bump;
  end;
  TPair<T> = record
    A: T;
    N: Integer;
  end;

procedure TBox<T>.Bump;
begin
  N := N + 1;
end;

procedure Put<T>(var P: TPair<T>; const V: T);
begin
  P.A := V;
  P.N := P.N + 1;
end;

var
  X: TBox<TA>;
  Y: TBox<TB>;
  P: TPair<Integer>;
  Q: TPair<Int64>;
  S: TPair<string>;
begin
  X.Bump;
  X.Bump;
  Y.Bump;
  Put<Integer>(P, 3);
  Put<Int64>(Q, 4);
  Put<string>(S, 'x');
  WriteLn(X.N, ' ', Y.N, ' ', P.A, ' ', P.N, ' ', Q.A, ' ', S.A, S.N);
end.
