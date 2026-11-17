// Names of the Pascal part of System.SysUtils are found through the full unit name, in a type,
// in raise and in on, as the names of the built-in table are (docs/spec/collections.md 1.2).
program StdlibSysUtilsQualified;

uses System.SysUtils;

var
  Y: System.SysUtils.EListError;
begin
  try
    raise System.SysUtils.EKeyNotFound.Create('k');
  except
    on E: System.SysUtils.EListError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  Y := System.SysUtils.EListError.Create('y');
  WriteLn(Y.Message);
  WriteLn(System.SysUtils.IntToStr(3));
end.
