program GenericsArityNames;

// A name with a different number of type parameters is another name (generics.md 2.3), so a
// generic declaration neither collides with a declaration without type parameters (no E0701) nor
// hides one (the built-in `TProc` stays visible, and its type mismatch is still reported).

type
  TBox<T> = record
    Value: T;
  end;
  TBox = Integer;
  TProc<T> = record
  end;

var
  B: TBox;
  Q: TProc;

begin
  B := 1;
  Q := 5; // E0702: `TProc` is still the built-in procedure type
  WriteLn(B);
end.
