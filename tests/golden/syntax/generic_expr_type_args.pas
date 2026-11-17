program GenericExprTypeArgs;

// In an expression, `IDENT <` starts type arguments when the tokens up to the matching `>` have
// the form of `TypeArgs` and the token after it cannot start a comparison's right operand
// (generics.md 1.4): TYPE_ARGS is the last child of NAME_REF / FIELD_EXPR. The names are not
// declared here, so the analysis reports them (E0700, E0761 for `A < B > (C)`).

var
  A, B, C, X: Integer;
  F: Boolean;
  O: TObject;
  P: TObject;
begin
  O := TList<Integer>.Create;
  X := Max<Integer>(1, 2);
  X := TUtil.Max<Double>(A, B);
  P := Identity<string>;
  X := Count<string> * 2;
  F := A < B > (C);
  O := TDict<string, TList<System.Integer>>.Create;
end.
