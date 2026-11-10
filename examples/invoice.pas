program Invoice;

uses
  System.SysUtils;

// Currency for money, case on strings, UTF8String and Unicode-aware case
// mapping.

type
  TLine = record
    Item: string;
    Price: Currency;
    Quantity: Integer;
  end;

function Line(const Item: string; Price: Currency; Quantity: Integer): TLine;
begin
  Result := Default(TLine);
  Result.Item := Item;
  Result.Price := Price;
  Result.Quantity := Quantity;
end;

function TaxRate(const Category: string): Currency;
begin
  case Category of
    'food', 'books': Result := 0.08;
    'gift': Result := 0;
  else
    Result := 0.10;
  end;
end;

var
  Lines: TArray<TLine>;
  Subtotal, Tax: Currency;
  Name: UTF8String;

begin
  Lines := [Line('Café au lait', 4.50, 2), Line('Notebook', 3.20, 3),
    Line('Straße map', 12.99, 1)];

  Subtotal := 0;
  for var L in Lines do
  begin
    WriteLn(Format('%-14s %2d x %6.2f', [L.Item, L.Quantity, L.Price]));
    Subtotal := Subtotal + L.Price * L.Quantity;
  end;
  Tax := Subtotal * TaxRate('food');
  WriteLn('Subtotal: ', Subtotal:0:2);
  WriteLn('Tax:      ', Tax:0:4);
  WriteLn('Total:    ', (Subtotal + Tax):0:2);

  // Currency keeps four decimal places exactly: 0.1 added ten times is 1.
  var Sum: Currency := 0;
  for var I := 1 to 10 do
    Sum := Sum + 0.1;
  WriteLn('Ten dimes = one: ', Sum = 1);

  // A UTF8String counts bytes, a string counts UTF-16 code units.
  Name := UTF8String(Lines[0].Item);
  WriteLn(string(Name), ': ', Length(Name), ' bytes, ', Length(Lines[0].Item), ' chars');
  WriteLn(UpperCase(Lines[0].Item), ' / ', AnsiUpperCase(Lines[0].Item));
  WriteLn(AnsiSameText('CAFÉ AU LAIT', Lines[0].Item));
end.
