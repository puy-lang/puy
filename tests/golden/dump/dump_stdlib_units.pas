// The declarations of the units of the standard library are not in the hir dump, as for any
// unit in uses; only the passed file is dumped (docs/spec/collections.md 12.3).
program DumpStdlibUnits;

uses System.SysUtils, System.Generics.Collections;

var
  P: TPair<string, Integer>;
begin
  P := TPair<string, Integer>.Create('a', 1);
  WriteLn(P.Key, IntToStr(P.Value));
end.
