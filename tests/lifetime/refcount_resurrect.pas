program RefcountResurrect;

uses System.SysUtils;

// A destructor that saves `Self` (resurrection) does not stop the destruction: the saved
// reference reads as a freed instance (`EAccessViolation`), and the tombstone left for it is
// not reported as a leak (objects.md 3.3).

type
  TPhoenix = class(TRefObject)
  public
    Name: string;
    destructor Destroy; override;
  end;

var
  Saved: TPhoenix;

destructor TPhoenix.Destroy;
begin
  WriteLn('destroy ', Name);
  Saved := Self;
  inherited Destroy;
end;

procedure Run;
var
  P: TPhoenix;
begin
  P := TPhoenix.Create;
  P.Name := 'phoenix';
  P := nil;
  WriteLn('saved: ', Assigned(Saved), Assigned(P));
  try
    WriteLn(Saved.Name);
  except
    on E: EAccessViolation do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end;

begin
  Run;
  WriteLn('end');
end.
