unit Shapes;

interface

function Area(W, H: Integer): Integer;

implementation

function Area(W, H: Integer): Integer;
begin
  Result := W * H;
end;

end.
