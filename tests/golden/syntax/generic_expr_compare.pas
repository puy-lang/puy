program GenericExprCompare;

// In an expression, `IDENT <` is a comparison when the tokens after `<` do not have the form of
// `TypeArgs` up to a `>`, or when the token after the `>` can start an expression
// (generics.md 1.4): no TYPE_ARGS here. `Zero<Integer> + 1` is `(Zero < Integer) > (+1)`.

var
  A, B, I: Integer;
  X: Boolean;
  Zero: Integer;
begin
  A := 1;
  B := 2;
  I := 0;
  Zero := 0;
  if A < B then
    X := A < B;
  while I < High(A) do
    I := High(A);
  X := Zero<Integer> + 1;
end.
