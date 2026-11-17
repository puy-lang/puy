program AttrRecordField;

// An attribute class on a field of a record (attributes.md 1.2) is accepted with no diagnostic,
// as on a field of a class, and does not change the program's output (4.3).

type
  MarkAttribute = class(TCustomAttribute)
  end;

  TPoint = record
    [Mark] X: Integer;
    [Mark, Mark] Y, Z: Integer;
  end;

var
  P: TPoint;
begin
  P.X := 1;
  P.Y := 2;
  P.Z := 3;
  WriteLn(P.X + P.Y + P.Z);
end.
