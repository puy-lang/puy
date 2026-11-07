program SysUtilsShort;

uses
  System.SysUtils;

begin
  WriteLn(IntToStr(12) + UpperCase('ab') + Trim('  c  '));
end.
