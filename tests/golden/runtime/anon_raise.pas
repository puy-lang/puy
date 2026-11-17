program AnonRaise;

uses System.SysUtils;

// Anonymous methods are called like routines (objects.md 9.6): `Exit` and `Result` are their own,
// an exception raised in one unwinds through its frame to the caller's handler (the stack trace
// names the function `MakeCheck.<anon1>`), and the values it captured are let go of as usual.

type
  TCheck = reference to function(N: Integer): Boolean;

function MakeCheck(const Limit: string): TCheck;
begin
  Result := function(N: Integer): Boolean
    begin
      Result := False;
      if N < 0 then
        Exit;
      if N > StrToInt(Limit) then
        raise ERangeError.Create('over ' + Limit);
      Result := True;
    end;
end;

var
  C: TCheck;
begin
  C := MakeCheck('10');
  WriteLn(C(-1), ' ', C(5));
  try
    C(11);
  except
    on E: ERangeError do
      WriteLn('caught: ', E.Message);
  end;
  C(12);
end.
