program ClassFields;

// Fields of class instances (objects.md 2.2, 2.7, 4.5): fields start at their default values,
// inherited fields come before the class's own, string / record / array fields are read and
// written through the instance, two variables holding the same instance see each other's
// writes, and `nil`, `Assigned`, `=` / `<>` compare references.

type
  TPoint = record
    X, Y: Integer;
  end;
  TShape = class
    Name: string;
    Origin: TPoint;
  end;
  TPolygon = class(TShape)
  private
    Sides: Integer;
  public
    Lengths: array of Integer;
    Corners: array[1..3] of TPoint;
    Next: TPolygon;
  end;

procedure Describe(S: TShape);
begin
  WriteLn(S.Name, ' at ', S.Origin.X, ',', S.Origin.Y);
end;

function Make(const AName: string; N: Integer): TPolygon;
begin
  Result := TPolygon.Create;
  Result.Name := AName;
  Result.Sides := N;
end;

var
  A, B: TPolygon;
  S: TShape;
  O: TObject;
  P: TPoint;
  I: Integer;
begin
  A := TPolygon.Create;
  WriteLn('[', A.Name, '] ', A.Sides, ' ', Length(A.Lengths), ' ', A.Origin.X, ' ', Assigned(A.Next));
  A.Name := 'triangle';
  A.Sides := 3;
  A.Origin.X := 10;
  A.Origin.Y := 20;
  SetLength(A.Lengths, 3);
  for I := 0 to 2 do
    A.Lengths[I] := (I + 1) * 5;
  A.Corners[2].Y := 7;
  A.Name := A.Name + '!';
  A.Name[1] := 'T';
  Inc(A.Sides);
  Describe(A);
  WriteLn(A.Sides, ' ', A.Lengths[0] + A.Lengths[1] + A.Lengths[2], ' ', A.Corners[2].Y);

  // A record read from a field is a copy
  P := A.Origin;
  P.X := 99;
  WriteLn(A.Origin.X, ' ', P.X);

  // Two variables, one instance
  B := A;
  B.Name := 'shared';
  B.Origin.Y := 21;
  WriteLn(A.Name, ' ', A.Origin.Y, ' ', A = B, ' ', A <> B);

  // An ancestor's variable and `TObject`
  S := A;
  O := A;
  WriteLn(S.Name, ' ', S = A, ' ', O = S, ' ', O <> nil);

  // A list through a field of the class's own type
  A.Next := Make('square', 4);
  A.Next.Next := Make('pentagon', 5);
  B := A;
  while B <> nil do
  begin
    WriteLn(B.Name, ' ', B.Sides);
    B := B.Next;
  end;
  WriteLn(Assigned(B), ' ', B = nil, ' ', Assigned(A.Next.Next));

  A.Next.Next.Free;
  A.Next.Free;
  A.Free;
  O := TObject.Create;
  WriteLn(O <> nil);
  O.Free;
end.
