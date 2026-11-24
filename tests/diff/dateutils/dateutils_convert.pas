program DateUtilsConvert;

// UNIX time of UTC values. (ISO 8601 text is in golden/runtime/dateutils_iso8601: Delphi XE4
// does not have DateToISO8601 and ISO8601ToDate.)

uses
  System.SysUtils, System.DateUtils;

procedure Show(const D: TDateTime);
begin
  WriteLn(FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', D));
end;

begin
  WriteLn(DateTimeToUnix(EncodeDate(1970, 1, 1)));
  WriteLn(DateTimeToUnix(EncodeDate(2026, 1, 1)));
  WriteLn(DateTimeToUnix(EncodeDateTime(2038, 1, 19, 3, 14, 8, 0)));
  WriteLn(DateTimeToUnix(EncodeDate(1969, 12, 31)));
  WriteLn(DateTimeToUnix(EncodeDate(1900, 1, 1)));
  Show(UnixToDateTime(0));
  Show(UnixToDateTime(1767225600));
  Show(UnixToDateTime(-86400));
  Show(UnixToDateTime(4102444800));
end.
