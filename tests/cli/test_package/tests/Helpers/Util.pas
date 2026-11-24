// A helper unit under tests/: its name is its path under tests/.
unit Helpers.Util;

interface

function Twice(N: Integer): Integer;

implementation

function Twice(N: Integer): Integer;
begin
  Result := N * 2;
end;

end.
