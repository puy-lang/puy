program ClassCastDiag;

// Diagnostics of `is`, `as` and conversions by type name (objects.md 2.8, 2.9, 16): E0702 for a
// class that can never be the other (neither descends from the other), for class references
// whose classes are unrelated, for `is` / `as` on a class reference (with a note on
// `InheritsFrom`) or on a value that is not an instance, for a right operand that is not a
// class, and for conversions between a class reference and an instance or an integer. An
// exception class is a class like the others (objects.md 8.1).

type
  TShape = class
  end;
  TCircle = class(TShape)
  end;
  TStream = class
  end;
  TShapeClass = class of TShape;
  TCircleClass = class of TCircle;
  TStreamClass = class of TStream;

var
  S: TShape;
  C: TCircle;
  T: TStream;
  K: TShapeClass;
  KS: TStreamClass;
  I: Integer;
  E: Exception;
  B: Boolean;
begin
  B := S is TStream;
  T := C as TStream;
  T := TStream(S);
  KS := TStreamClass(K);
  K := TShapeClass(KS);
  B := K is TShape;
  K := K as TCircle;
  B := I is TShape;
  B := S is Integer;
  B := S is K;
  B := S is TCircleClass;
  B := E is Exception;
  B := S is Exception;
  I := Integer(K);
  K := TShapeClass(I);
  K := TShapeClass(S);
  S := TShape(K);
  // To an ancestor, and the same class
  B := C is TShape;
  S := C as TShape;
  S := TShape(S);
  K := TShapeClass(K);
end.
