program GenericsRecovery;

// Error recovery in type parameters (generics.md 1.6): a missing `>` is supplied at the end of
// the declaration, a type parameter's name and a constraint are asked for, and the reading goes
// on with the next declaration.

type
  TMissing<T = class
  end;
  TNoName<1> = class
  end;
  TBadConstraint<T: 42; U: class, 'x'> = class
  end;

procedure Heading<T(X: Integer);
begin
end;

var
  V: TList<Integer;

begin
end.
