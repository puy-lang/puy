unit Counter;

interface

function Next: Integer;

implementation

var
  N: Integer;

function Next: Integer;
begin
  N := N + 1;
  Result := N;
end;

end.
