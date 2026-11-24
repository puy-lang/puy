// puy-test: test-ext
// The host and extension functions only a dropped function uses are not in the module
// (units.md 6.1): `Unused` is lowered first and uses `Console.WriteError` and `Half`, and the
// others are numbered in order of first use in the kept functions (`Negate` before `Add`).
program DumpUnreachableExt;

uses TestExt;

procedure Unused;
begin
  WriteLn(ErrOutput, Half(1.5));
  WriteLn(Add(1, 2));
end;

begin
  WriteLn(Negate(False));
  WriteLn(Add(1, 2));
end.
