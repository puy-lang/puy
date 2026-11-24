// puy-test: clock=1795178096789123456, tz-offset=32400
program DateUtilsLocal;

// The routines of System.DateUtils that read the clock or the local time zone (here UTC+9):
// today, ISO 8601 text and UNIX time of local values.

uses
  System.SysUtils, System.DateUtils;

procedure Show(const D: TDateTime);
begin
  WriteLn(FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', D));
end;

begin
  Show(Today);
  Show(Yesterday);
  Show(Tomorrow);
  WriteLn(IsToday(Now), ' ', IsToday(Tomorrow));
  WriteLn(DateToISO8601(EncodeDateTime(2026, 11, 20, 21, 34, 56, 789), False));
  WriteLn(DateToISO8601(EncodeDate(1, 1, 1), False));
  Show(ISO8601ToDate('2026-11-20T12:34:56Z', False));
  Show(ISO8601ToDate('2026-11-20T12:34:56-05:00', False));
  Show(ISO8601ToDate('2026-11-20T12:34:56', False));
  Show(ISO8601ToDate('2026-11-20T12:34:56'));
  WriteLn(DateTimeToUnix(EncodeDate(1970, 1, 1), False));
  WriteLn(DateTimeToUnix(EncodeDate(1970, 1, 1)));
  Show(UnixToDateTime(0, False));
  Show(UnixToDateTime(1795178096, False));
end.
