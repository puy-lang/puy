// puy-test: no-clock
program DateTimeNoClock;

// Without a clock, Now and TInstant.Now raise ENotSupportedException with the kind of the
// absence; the conversions that do not read the clock still work.

uses
  System.SysUtils;

begin
  try
    WriteLn(DateTimeToStr(Now));
  except
    on E: ENotSupportedException do
      WriteLn(E.ClassName, ': ', E.Message, ' ', E.ErrorKind);
  end;
  try
    WriteLn(TInstant.Now.ToString);
  except
    on E: ENotSupportedException do
      WriteLn(E.ClassName, ': ', E.Message, ' ', E.ErrorKind);
  end;
  WriteLn(TInstant.FromUnixSeconds(0).ToString, ' ', DateToStr(EncodeDate(2026, 1, 2)));
end.
