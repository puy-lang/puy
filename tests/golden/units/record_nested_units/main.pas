program Main;

// A type nested in a record of another unit is named through the unit and the record
// (`Shapes.TShape.TPoint`; objects.md 2.2, generics.md 2.5), and one nested in an
// instantiation of that unit's generic record after the type arguments.

uses Shapes;

var
  P: Shapes.TShape.TPoint;
  Points: Shapes.TShape.TPoints;
  C: TBox<Integer>.TCell;
begin
  P := Shapes.TShape.TPoint.Create(1, 2);
  Points := [P, P];
  C.Value := Shapes.TShape.Limit;
  WriteLn(P.Sum, ' ', Length(Points), ' ', C.Value);
end.
