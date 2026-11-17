program CtorExceptionSavedSelf;

// A constructor of a reference counted class that saves `Self` outside and then raises
// (objects.md 3.1, 3.3, ch18): the instance is destroyed (the destructor runs, `BeforeDestruction`
// does not), the variable assigned keeps `nil`, and the reference left outside is a freed handle:
// using it is EAccessViolation, and it is not reported as a leak.

uses
  System.SysUtils;

type
  TItem = class(TRefObject)
    Value: Integer;
    constructor Create(A: Integer);
    destructor Destroy; override;
    procedure BeforeDestruction; override;
  end;

var
  Last: TItem;

constructor TItem.Create(A: Integer);
begin
  Value := A;
  Last := Self;
  raise Exception.Create('fail');
end;

destructor TItem.Destroy;
begin
  WriteLn('destroy ', Value);
  inherited Destroy;
end;

procedure TItem.BeforeDestruction;
begin
  WriteLn('before destruction');
end;

var
  X: TItem;
begin
  X := nil;
  try
    X := TItem.Create(7);
  except
    on E: Exception do
      WriteLn('caught ', E.Message);
  end;
  WriteLn(Assigned(X), ' ', Assigned(Last));
  try
    WriteLn(Last.Value);
  except
    on E: EAccessViolation do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end.
