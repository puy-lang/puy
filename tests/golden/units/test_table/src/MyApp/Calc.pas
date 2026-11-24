unit MyApp.Calc;

interface

function Twice(X: Integer): Integer;

implementation

function Half(X: Integer): Integer;
begin
  Result := X div 2;
end;

function Twice(X: Integer): Integer;
begin
  Result := X * 2;
end;

end.
