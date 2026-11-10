program Exceptions;

uses
  System.SysUtils;

// Raising, catching and cleaning up with try..except and try..finally.

type
  EInsufficientFunds = class(Exception);

var
  Balance: Integer = 100;

procedure Withdraw(Amount: Integer);
begin
  if Amount > Balance then
    raise EInsufficientFunds.CreateFmt('cannot withdraw %d (balance %d)', [Amount, Balance]);
  Dec(Balance, Amount);
end;

procedure Attempt(const Input: string);
begin
  try
    try
      Withdraw(StrToInt(Input));
      WriteLn('withdrew ', Input, ', balance ', Balance);
    finally
      WriteLn('  (checked "', Input, '")');
    end;
  except
    on E: EInsufficientFunds do
      WriteLn('refused: ', E.Message);
    on E: EConvertError do
      WriteLn('bad input: ', E.Message);
  end;
end;

begin
  Attempt('30');
  Attempt('500');
  Attempt('abc');
  Attempt('70');
end.
