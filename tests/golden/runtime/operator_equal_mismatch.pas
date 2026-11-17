program OperatorEqualMismatch;

// A reference counted class whose `Equal` does not take the operands being compared
// (objects.md 11.3, 11.6): `=` and `<>` both compare the references, since `<>` is E0703 only
// when the type's `Equal` would take the operands.

type
  TFoo = class(TRefObject)
    V: Integer;
    class operator Equal(const A: TFoo; B: Integer): Boolean;
  end;

class operator TFoo.Equal(const A: TFoo; B: Integer): Boolean;
begin
  Result := A.V = B;
end;

var
  X, Y: TFoo;
begin
  X := TFoo.Create;
  Y := TFoo.Create;
  WriteLn(X = Y, ' ', X <> Y, ' ', X = X, ' ', X <> X, ' ', X = 0);
end.
