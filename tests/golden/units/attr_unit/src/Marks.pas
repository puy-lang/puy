unit Marks;

// The attributes of a routine are written on its heading in the interface section; the body's
// heading may not have them (E3401, pointing at the heading). An attribute class of the unit is
// used from the program by its name, with or without the unit's name.

interface

type
  MarkAttribute = class(TCustomAttribute)
  end;

[Mark]
procedure Greet;

[Mark]
procedure Wave;

implementation

procedure Greet;
begin
  WriteLn('hello');
end;

[Mark]
procedure Wave;
begin
  WriteLn('wave');
end;

end.
