program DumpRecordMethods;

// The three ways `Self` of a record method is passed (objects.md chapter 6): a variable by
// reference (`MakeRef`), an element taken out and put back (`Lend` / `Unlend`), and a `const`
// parameter or a function's result copied to a temporary whose reference is passed; and a record
// constructor running on a temporary that becomes the value.

type
  TPoint = record
    X, Y: Integer;
    constructor Create(AX, AY: Integer);
    procedure Offset(DX: Integer);
    function Sum: Integer;
    property PX: Integer read X write X;
  end;

constructor TPoint.Create(AX, AY: Integer);
begin
  X := AX;
  Y := AY;
end;

procedure TPoint.Offset(DX: Integer);
begin
  X := X + DX;
end;

function TPoint.Sum: Integer;
begin
  Result := X + Y;
end;

function Make: TPoint;
begin
  Result := TPoint.Create(1, 2);
end;

procedure Run(const C: TPoint);
var
  V: TPoint;
  A: array of TPoint;
begin
  V := C;
  SetLength(A, 1);
  V.Offset(1);
  A[0].Offset(2);
  C.Offset(3);
  Make.Offset(4);
  WriteLn(V.Sum, ' ', A[0].PX);
end;

begin
  Run(TPoint.Create(5, 6));
end.
