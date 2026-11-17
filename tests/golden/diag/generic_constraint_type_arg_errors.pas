program GenericConstraintTypeArgErrors;

// Constraints written with type arguments (generics.md 1.1, 3.1, 3.5, 9): a type argument is
// checked against the constraint with the type parameters replaced, and E0759 names the
// constraint so (`IComparable<Integer>`), for a type, a routine (written and inferred type
// arguments) and a method whose constraint has its class's type parameter. A type parameter
// passed on without the constraint gets the constraint to add in the notes. The instantiations
// of a `type` section are reported once each when its classes are complete, and stay types (no
// E0723 between a method's declaration and its body). A type with type arguments that is not a
// class or an interface is not a constraint (E0758).

type
  IComparable<T> = interface
    function CompareTo(const Other: T): Integer;
  end;

  IKeyed<K> = interface
    function Key: K;
  end;

  TNum = class(TRefObject, IComparable<TNum>)
    function CompareTo(const Other: TNum): Integer;
  end;

  // Comparable with `TNum`, not with itself
  TOther = class(TRefObject, IComparable<TNum>)
    function CompareTo(const Other: TNum): Integer;
  end;

  TSorted<T: IComparable<T>> = class
  end;

  TBy<K; V: IKeyed<K>> = class
  end;

  TNode<T: TNode<T>> = class
  end;

  TBad<T: TArray<T>> = class
  end;

  TPlain<T> = class
    procedure Use;
  end;

  TBox<T> = class
    function Pick<C: IComparable<T>>(const Cmp: C): T;
  end;

  // In a `type` section the checks wait for the end of the section (generics.md 4.2): each is
  // still reported once, at its type argument
  TUser = class(TSorted<TOther>)            // E0759 in a parent
    F: TBy<Integer, TNum>;                  // E0759 in a field
    procedure P(const X: TSorted<Integer>); // E0759 in a method heading
  end;

function TNum.CompareTo(const Other: TNum): Integer;
begin
  Result := 0;
end;

function TOther.CompareTo(const Other: TNum): Integer;
begin
  Result := 0;
end;

procedure TPlain<T>.Use;
begin
  WriteLn(TSorted<T>.ClassName); // E0759: `T` has no constraint
end;

function TBox<T>.Pick<C>(const Cmp: C): T;
begin
  Result := Default(T);
end;

procedure TUser.P(const X: TSorted<Integer>);
begin
end;

function Largest<T: IComparable<T>>(const A, B: T): T;
begin
  Result := A;
end;

var
  A: TSorted<Integer>;     // E0759
  B: TSorted<TOther>;      // E0759: `IComparable<TOther>`
  C: TBy<Integer, TNum>;   // E0759: `IKeyed<Integer>`
  D: TNode<TNum>;          // E0759: `TNode<TNum>`
  Box: TBox<Integer>;
begin
  Largest<TOther>(TOther.Create, TOther.Create); // E0759
  Largest(TOther.Create, TOther.Create);         // E0759, inferred
  Box.Pick<TNum>(TNum.Create);                   // E0759: `IComparable<Integer>`
end.
