// puy-test: clock=0, mono=18446744073709551615, mono-step=0
program VirtualClockMonoRange;

// The virtual monotonic clock is an unsigned 64-bit count: frozen at its last value, Sleep and
// Advance past it raise ERangeError and move neither clock, so a stopwatch stays where it was.

uses
  System.SysUtils, System.Diagnostics, Puy.Testing;

begin
  TTestClock.Freeze;
  var SW := TStopwatch.StartNew;
  try
    Sleep(1);
  except
    on E: ERangeError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    TTestClock.Advance(1);
  except
    on E: ERangeError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  WriteLn(TInstant.Now.ToString, ' ', SW.ElapsedTicks);
end.
