unit Shapes;

interface

type
  TShape = record
  type
    TPoint = record
      X, Y: Integer;
      constructor Create(AX, AY: Integer);
      function Sum: Integer;
    end;
    TPoints = array of TPoint;
  const
    Limit = 10;
  end;

  TBox<T> = record
  type
    TCell = record
      Value: T;
    end;
  end;

implementation

constructor TShape.TPoint.Create(AX, AY: Integer);
begin
  X := AX;
  Y := AY;
end;

function TShape.TPoint.Sum: Integer;
begin
  Result := X + Y;
end;

end.
