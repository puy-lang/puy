unit Report_test;

interface

implementation

uses
  System.SysUtils, Puy.Testing, Report;

// The paired Report_test unit can call Report's implementation routine TotalWeight.
[TestCase(3, 2.50, 7.50)]
[TestCase(3, 0.10, 0.30)]
procedure CalculatesTotal(Quantity: Integer; UnitWeight, Expected: Double);
begin
  AssertNear(TotalWeight(Quantity, UnitWeight), Expected, 0.000001);
end;

[Test]
procedure RejectsZeroQuantity;
begin
  var Error := AssertRaises<EArgumentException>(
    procedure
    begin
      RenderReport('Boxes', 0, 2.50);
    end);
  Assert(Error.Message = 'quantity must be positive');
end;

[Test]
procedure PrintsReport;
begin
  AssertSnapshot(RenderReport('Boxes', 3, 2.50));
end;

end.
