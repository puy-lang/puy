// puy-test: clock=1795178096789123456, tz-offset=-16200
program DateUtilsLocalWest;

// DateToISO8601 of a local time west of UTC (here UTC-4:30) writes a negative offset; the other
// conversions move the other way.

uses
  System.SysUtils, System.DateUtils;

begin
  WriteLn(DateToISO8601(EncodeDateTime(2026, 11, 20, 8, 4, 56, 789), False));
  WriteLn(FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', ISO8601ToDate('2026-11-20T12:34:56Z', False)));
  WriteLn(DateTimeToUnix(EncodeDate(1970, 1, 1), False));
  WriteLn(FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', Today));
end.
