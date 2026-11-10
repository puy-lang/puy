program Temperature;

uses
  System.SysUtils;

// Floating-point arithmetic, overloads and default parameters.

function ToFahrenheit(Celsius: Double): Double;
begin
  Result := Celsius * 9 / 5 + 32;
end;

function Describe(Value: Double; const Unit_: string = 'C'): string; overload;
begin
  Result := Format('%.1f %s', [Value, Unit_]);
end;

function Describe(Value: Int64): string; overload;
begin
  Result := IntToStr(Value) + ' (whole)';
end;

var
  Total: Double;
  Count: Integer;

begin
  WriteLn('Celsius  Fahrenheit');
  Total := 0;
  Count := 0;
  var C := -10.0;
  while C <= 40 do
  begin
    WriteLn(C:7:1, ToFahrenheit(C):12:1);
    Total := Total + C;
    Inc(Count);
    C := C + 12.5;
  end;

  WriteLn('Average: ', Describe(Total / Count));
  WriteLn('Rounded: ', Describe(Round(Total / Count)));
  WriteLn('Body:    ', Describe(ToFahrenheit(37), 'F'));
  WriteLn('Sqrt(2) = ', Sqrt(2):0:6, ', Pi = ', Pi:0:6);
end.
