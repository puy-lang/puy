// A unit of the package found from the working directory.
unit Calc;

interface

function Add(A, B: Integer): Integer;

implementation

const
  Secret = 1;

function Add(A, B: Integer): Integer;
begin
  Result := A + B;
end;

end.
