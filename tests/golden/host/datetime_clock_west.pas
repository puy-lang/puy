// puy-test: clock=1798747200000000000, tz-offset=-18000
program DateTimeClockWest;

// An offset west of UTC moves the local date back: 2026-12-31T20:00Z is 15:00 the same day at
// UTC-5, and Date has no time.

uses
  System.SysUtils;

begin
  WriteLn(DateTimeToStr(Now));
  WriteLn(DateToStr(Date), ' ', FloatToStr(Date - Trunc(Date)));
end.
