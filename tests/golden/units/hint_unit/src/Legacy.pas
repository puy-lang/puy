unit Legacy deprecated 'use Modern';

// A unit marked `deprecated` (attributes.md 6.2): the warning is on the `uses` item only, and
// uses inside the unit of its own `deprecated` declarations are not reported (the unit has the
// same mark); an `experimental` declaration used inside it still is.

interface

const
  Limit = 3;

procedure OldRun; deprecated 'use Run';
procedure Trial; experimental;

implementation

procedure Trial;
begin
  WriteLn('trial');
end;

procedure OldRun;
begin
  Trial;
end;

procedure Helper;
begin
  OldRun;
end;

initialization
  Helper;
end.
