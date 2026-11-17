program CtorSavedSelf;

// A constructor that saved `Self` outside before failing (objects.md 3.1): the instance is
// destroyed all the same (its destructor runs), so the saved reference is a destroyed handle
// and reading through it raises EAccessViolation.

uses
  System.SysUtils;

type
  TItem = class
  public
    Value: Integer;
    constructor Create(AValue: Integer);
    destructor Destroy; override;
  end;

var
  Last: TItem;

constructor TItem.Create(AValue: Integer);
begin
  Value := AValue;
  Last := Self;
  if AValue < 0 then
    raise Exception.Create('negative value');
end;

destructor TItem.Destroy;
begin
  WriteLn('TItem.Destroy ', Value);
  inherited;
end;

var
  X: TItem;
begin
  X := nil;
  try
    X := TItem.Create(-1);
  except
    on E: Exception do
      WriteLn('caught: ', E.Message);
  end;
  WriteLn(Assigned(X));
  WriteLn(Assigned(Last));
  try
    WriteLn(Last.Value);
  except
    on E: EAccessViolation do
      WriteLn('EAccessViolation: ', E.Message);
  end;
end.
