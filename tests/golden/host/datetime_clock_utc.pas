// puy-test: clock=1798747200000000000
program DateTimeClockUtc;

// A host whose clock cannot tell the offset of the local time (WASI, an embedding) gives UTC.

uses
  System.SysUtils;

begin
  WriteLn(DateTimeToStr(Now));
  WriteLn(TInstant.Now.ToString);
end.
