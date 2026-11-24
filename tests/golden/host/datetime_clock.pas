// puy-test: clock=1795178096789123456, tz-offset=32400
program DateTimeClock;

// Now, Date and Time read the host's clock in the local time (here UTC+9); TInstant.Now reads it
// in UTC, to the nanosecond. Now is to the millisecond.

uses
  System.SysUtils;

begin
  WriteLn(FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', Now));
  WriteLn(DateTimeToStr(Date), ' | ', TimeToStr(Time));
  WriteLn(TInstant.Now.ToString);
  WriteLn(TInstant.Now.ToUnixNanos);
  WriteLn(FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', TInstant.Now.ToDateTime));
end.
