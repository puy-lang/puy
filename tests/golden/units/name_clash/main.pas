// A name in two units of uses is E2502; System.SysUtils counts, System does not; the program
// cannot have the name of a unit (docs/spec/units.md 3.1, 4.3).
program Shared;

uses Shared, Other, System.SysUtils;

begin
  WriteLn(MaxInt);
  WriteLn(Limit);
  WriteLn(IntToStr(1));
  WriteLn(Shared.Limit, Other.Limit);
end.
