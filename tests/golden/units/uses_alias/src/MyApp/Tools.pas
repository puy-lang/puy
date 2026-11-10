unit MyApp.Tools;

interface

function Twice(N: Integer): Integer;

implementation

function Twice(N: Integer): Integer;
begin
  Result := N * 2;
end;

end.
