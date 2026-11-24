unit Report;

interface

function RenderReport(const Item: string; Quantity: Integer; UnitWeight: Double): string;

implementation

uses
  System.SysUtils;

function TotalWeight(Quantity: Integer; UnitWeight: Double): Double;
begin
  if Quantity < 1 then
    raise EArgumentException.Create('quantity must be positive');
  Result := Quantity * UnitWeight;
end;

function RenderReport(const Item: string; Quantity: Integer; UnitWeight: Double): string;
begin
  Result := Format('%s x%d'#10'Total weight: %.2f kg'#10,
    [Item, Quantity, TotalWeight(Quantity, UnitWeight)]);
end;

end.
