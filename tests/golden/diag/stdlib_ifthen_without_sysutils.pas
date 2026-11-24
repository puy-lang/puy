// IfThen is declared in System.SysUtils, not in System.Math or System.StrUtils, so that the
// numbers and the strings share one overload set: without System.SysUtils it is E0700 with the
// fix that adds the unit (docs/spec/stdlib/System.SysUtils/IfThen.md).
program StdlibIfThenWithoutSysUtils;

uses System.Math, System.StrUtils;

begin
  WriteLn(IfThen(True, 'a', 'b'), Max(1, 2));
end.
