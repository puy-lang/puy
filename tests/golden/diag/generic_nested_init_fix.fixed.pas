program GenericNestedInitFix;

// The fix of E2000 (an initial value with `Default(…)`) for variables of generic types nested in
// a type (generics.md 2.5): none for an instantiation of a type nested in a generic type with the
// outer type's type parameters (`TOuter<T>.TPair<Integer>` cannot be written by its name), and
// `Default(THolder.TBox<Integer>)` for a generic type nested in a class, which the fixed file
// resolves.

type
  TOuter<T> = class
  public
    type
      TPair<U> = record
        A: T;
        B: U;
      end;
    function Make(const X: T): Integer;
  end;

  THolder = class
  public
    type
      TBox<T> = record
        V: T;
      end;
  end;

function TOuter<T>.Make(const X: T): Integer;
var
  P: TPair<Integer>;
begin
  P.A := X;
  Result := P.B;
end;

procedure Run;
var
  B: THolder.TBox<Integer> = Default(THolder.TBox<Integer>);
begin
  B.V := B.V + 1;
  WriteLn(B.V);
end;

begin
  Run;
end.
