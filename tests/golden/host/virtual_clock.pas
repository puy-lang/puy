// puy-test: clock=1795178096789123456, tz-offset=32400, mono=5000000000, mono-step=1500000
program VirtualClock;

// TTestClock (Puy.Testing) freezes the isolate's clock: Now, TInstant.Now, TStopwatch and Sleep
// then read the virtual clock, which moves only on Advance, SetNow and Sleep, while the host's
// test clock would have moved 1.5 ms at each read of the monotonic clock. Freeze on a frozen
// clock keeps it; Freeze(At) on a frozen clock sets the wall clock only. Advance and SetNow need a
// frozen clock; a negative time and a move past the range raise.

uses
  System.SysUtils, System.Diagnostics, Puy.Testing;

procedure Show(const Step: string; const SW: TStopwatch);
begin
  WriteLn(Step, ': ', TInstant.Now.ToString, ' ', FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', Now),
    ' ', SW.ElapsedTicks);
end;

procedure Fails(const Step: string; const P: TProc);
begin
  try
    P;
    WriteLn(Step, ': no exception');
  except
    on E: Exception do
      WriteLn(Step, ': ', E.ClassName, ': ', E.Message);
  end;
end;

begin
  Fails('advance before freeze', procedure begin TTestClock.Advance(1) end);
  Fails('set before freeze', procedure begin TTestClock.SetNow(TInstant.FromUnixSeconds(0)) end);
  var Real := TStopwatch.StartNew;
  WriteLn('real: ', Real.ElapsedTicks, ' ', Real.ElapsedTicks);
  TTestClock.Freeze;
  var SW := TStopwatch.StartNew;
  Show('frozen', SW);
  Show('still', SW);
  TTestClock.Freeze;
  Show('freeze again', SW);
  TTestClock.Advance(1500);
  Show('advance 1500', SW);
  Sleep(250);
  Show('sleep 250', SW);
  Sleep(0);
  Show('sleep 0', SW);
  TTestClock.SetNow(TInstant.Parse('2000-01-01T00:00:00Z'));
  Show('set 2000', SW);
  TTestClock.Freeze(TInstant.Parse('2030-06-15T12:00:00.5Z'));
  Show('freeze at 2030', SW);
  WriteLn('stopwatch from before: ', Real.ElapsedTicks);
  Fails('advance -1', procedure begin TTestClock.Advance(-1) end);
  Fails('advance max', procedure begin TTestClock.Advance(High(Int64)) end);
  TTestClock.SetNow(TInstant.FromUnixNanos(High(Int64) - 1000000));
  Fails('advance past the range', procedure begin TTestClock.Advance(2) end);
  Fails('sleep past the range', procedure begin Sleep(2) end);
  WriteLn('unmoved: ', TInstant.Now.ToUnixNanos, ' ', SW.ElapsedTicks);
  TTestClock.Advance(1);
  WriteLn('to the end: ', TInstant.Now.ToUnixNanos = High(Int64), ' ', SW.ElapsedTicks);
end.
