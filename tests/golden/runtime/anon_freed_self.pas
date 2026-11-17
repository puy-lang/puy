program AnonFreedSelf;

// An anonymous method in a method of a class managed by hand holds `Self` without counting it
// (objects.md 9.6): called while the instance lives it works, called after `Free` it raises
// `EAccessViolation`, and the program ends.

type
  TPlain = class
    Name: string;
    function Greeting: TProc;
  end;

function TPlain.Greeting: TProc;
begin
  Result := procedure
    begin
      WriteLn('hello ', Name);
    end;
end;

var
  M: TPlain;
  P: TProc;
begin
  M := TPlain.Create;
  M.Name := 'm';
  P := M.Greeting();
  P();
  M.Free;
  P();
  WriteLn('not reached');
end.
