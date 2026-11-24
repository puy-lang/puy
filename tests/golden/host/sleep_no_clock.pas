// puy-test: no-clock, absent=unsupported
program SleepNoClock;

// Without a clock, Sleep raises ENotSupportedException as Now does, and so does Freeze, which
// reads the real time; Freeze(At) needs no clock, and the virtual clock then serves Sleep and
// TInstant.Now.

uses
  System.SysUtils, Puy.Testing;

begin
  try
    Sleep(1);
  except
    on E: ENotSupportedException do
      WriteLn(E.ClassName, ': ', E.Message, ' ', E.ErrorKind);
  end;
  try
    TTestClock.Freeze;
  except
    on E: ENotSupportedException do
      WriteLn(E.ClassName, ': ', E.Message, ' ', E.ErrorKind);
  end;
  TTestClock.Freeze(TInstant.FromUnixSeconds(60));
  Sleep(1000);
  WriteLn(TInstant.Now.ToString);
end.
