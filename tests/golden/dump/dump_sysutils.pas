program DumpSysUtils;

// The hir and bytecode dumps of the M2b System.SysUtils built-ins (semantics.md 9.1,
// bytecode.md 2.16): Format with the elements of its `array of const` (pairs of kind and value),
// TryStrToInt with its `out` argument, BoolToStr with the omitted argument, IntToHex of a narrow
// type, StringReplace with TReplaceFlags and Exception.CreateFmt.

uses System.SysUtils;

var
  V: Integer;
  S: ShortInt;
  U: UTF8String;
begin
  S := -1;
  U := 'u';
  WriteLn(Format('%d %s %s %g %s', [V, 'a', U, 1.5, 'c']));
  if TryStrToInt('12', V) then
    WriteLn(V);
  WriteLn(BoolToStr(V > 0), IntToHex(S, 2));
  WriteLn(StringReplace('aXa', 'x', '-', [TReplaceFlag.rfReplaceAll, TReplaceFlag.rfIgnoreCase]));
  raise Exception.CreateFmt('%d', [V]);
end.
