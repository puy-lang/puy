program Exceptions;

uses
  System.SysUtils;

// Raising, catching and cleaning up with try..except and try..finally.
// Exception classes are ordinary classes: they can have fields, properties
// and their own constructors.

type
  EInsufficientFunds = class(Exception)
  private
    FShortfall: Integer;
  public
    constructor Create(Amount, Balance: Integer);
    property Shortfall: Integer read FShortfall;
  end;

  TAccount = class
  private
    FBalance: Integer;
  public
    constructor Create(Opening: Integer);
    procedure Withdraw(Amount: Integer);
    property Balance: Integer read FBalance;
  end;

constructor EInsufficientFunds.Create(Amount, Balance: Integer);
begin
  inherited CreateFmt('cannot withdraw %d (balance %d)', [Amount, Balance]);
  FShortfall := Amount - Balance;
end;

constructor TAccount.Create(Opening: Integer);
begin
  inherited Create;
  FBalance := Opening;
end;

procedure TAccount.Withdraw(Amount: Integer);
begin
  if Amount > FBalance then
    raise EInsufficientFunds.Create(Amount, FBalance);
  Dec(FBalance, Amount);
end;

procedure Attempt(Account: TAccount; const Input: string);
begin
  try
    try
      Account.Withdraw(StrToInt(Input));
      WriteLn('withdrew ', Input, ', balance ', Account.Balance);
    finally
      WriteLn('  (checked "', Input, '")');
    end;
  except
    on E: EInsufficientFunds do
      WriteLn('refused: ', E.Message, ', short by ', E.Shortfall);
    on E: EConvertError do
      WriteLn('bad input: ', E.Message);
  end;
end;

var
  Account: TAccount;

begin
  Account := TAccount.Create(100);
  try
    Attempt(Account, '30');
    Attempt(Account, '500');
    Attempt(Account, 'abc');
    Attempt(Account, '70');
  finally
    Account.Free;
  end;
end.
