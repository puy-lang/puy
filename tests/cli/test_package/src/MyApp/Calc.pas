// The unit under test: its `implementation` section is seen by its test unit.
unit MyApp.Calc;

interface

function Add(A, B: Integer): Integer;

implementation

const
  Offset = 0;

function Add(A, B: Integer): Integer;
begin
  Result := A + B + Offset;
end;

end.
