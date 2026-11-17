// Names of the Pascal part of System.SysUtils are found through an alias, in a type, in raise
// and in on, as the names of the built-in table are (docs/spec/collections.md 1.2).
program StdlibSysUtilsAlias;

uses System.SysUtils as SU, System.Generics.Collections;

var
  Y: SU.EKeyNotFound;
begin
  try
    raise SU.EKeyNotFound.Create('k');
  except
    on E: SU.EListError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  Y := SU.EKeyNotFound.Create('y');
  WriteLn(Y.Message);
  WriteLn(SU.IntToStr(3));
end.
