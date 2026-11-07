program SysUtilsShort;

uses
  SysUtils;

begin
  WriteLn(IntToStr(12) + UpperCase('ab') + Trim('  c  '));
end.
