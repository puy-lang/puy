// A unit in a file named `*_test.pas` is a test unit without a package too (units.md 2.4): it
// takes the attributes of the testing framework, and no `initialization` section (E0780). Without
// a package it has no paired unit.
unit Lone_test;

interface

implementation

[Test]
procedure Works;
begin
  Assert(True);
end;

begin
end.
