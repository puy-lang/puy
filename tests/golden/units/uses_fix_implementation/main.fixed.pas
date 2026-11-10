// The E0700 fix inserts a uses clause after `implementation` when the section has none
// (docs/spec/units.md 3.1).
unit Main;

interface

uses Helpers;

function Square(N: Integer): Integer;

implementation
uses Shapes;

function Square(N: Integer): Integer;
begin
  Result := Area(N, N);
end;

end.
