program EarlyRelease;

// Which temporaries are let go of before the routine ends (lifetime.md 3.4): a dynamic array
// holding no object is released at the end of the statement that read it (an `if` condition's
// on both paths before the bodies), while a string and an array of exceptions keep their
// reference until the routine ends.

uses System.SysUtils;

var
  A: array of Integer;
  E: array of Exception;
  S: string;

procedure Run;
begin
  WriteLn(Length(A), ' ', Length(E), ' ', S);
  if Length(A) = 2 then
    WriteLn('two')
  else
    WriteLn('other');
end;

begin
  SetLength(A, 2);
  S := 'x';
  Run;
end.
