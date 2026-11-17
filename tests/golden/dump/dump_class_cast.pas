program DumpClassCast;

// The dumps of `is`, `as` and conversions by type name (objects.md 14.3, 15.1, 15.2): `is` to a
// descendant is `IsClass` on an uncounted copy, `as` and `TCircle(S)` are `AsClass` and a class
// reference's conversion is the intrinsic `AsClassRef` with the type index; assigned to a
// variable, the check runs on a temporary first. `is` to an ancestor is `<> nil` and `as` to an
// ancestor is nothing.

type
  TShape = class
  end;
  TCircle = class(TShape)
    Radius: Integer;
  end;
  TShapeClass = class of TShape;
  TCircleClass = class of TCircle;

procedure Check(S: TShape; K: TShapeClass);
var
  C: TCircle;
  CK: TCircleClass;
  B: Boolean;
begin
  B := S is TCircle;
  C := S as TCircle;
  C := TCircle(S);
  CK := TCircleClass(K);
  WriteLn((S as TCircle).Radius, TCircle(S).Radius, S is TCircle);
  B := B and (C is TShape);
  S := C as TShape;
  WriteLn(B, S = nil, CK = nil);
end;

var
  X: TShape;
begin
  X := TCircle.Create;
  Check(X, TCircle);
  X.Free;
end.
