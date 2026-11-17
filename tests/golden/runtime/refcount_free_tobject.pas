program RefcountFreeTObject;

uses System.SysUtils;

// `Free` of a variable of type exactly `TObject` holding an instance of a reference counted
// class is `EInvalidOperation` (objects.md 3.2); the instance lives on and is destroyed when
// its last reference goes.

type
  TCounted = class(TRefObject)
  public
    destructor Destroy; override;
  end;

destructor TCounted.Destroy;
begin
  WriteLn('destroy');
  inherited Destroy;
end;

procedure Run;
var
  O: TObject;
begin
  O := TCounted.Create;
  try
    O.Free;
  except
    on E: EInvalidOperation do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  WriteLn('still assigned: ', Assigned(O));
  O.Destroy;
end;

begin
  Run;
end.
