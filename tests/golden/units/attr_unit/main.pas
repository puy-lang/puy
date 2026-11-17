// Attributes across units (attributes.md 1.2, 2): the body of a routine of the interface
// section with attributes is E3401; an attribute class of a used unit is found by its name and by
// the name qualified with the unit (`[Marks.Mark]`).
program Main;

uses Marks;

[Mark]
procedure Run;
begin
  Greet;
  Wave;
end;

[Marks.Mark]
procedure Again;
begin
  Run;
end;

begin
  Again;
end.
