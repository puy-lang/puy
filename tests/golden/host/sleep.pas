// puy-test: clock=0, mono=1000, mono-step=0
program SleepCase;

// Sleep on the real clock waits on the host's clock (the test clock moves by the time asked
// instead of waiting): the wall clock and the monotonic clock both move. Sleep(0) does not move
// them. Standard output is written out before the wait.

uses
  System.SysUtils, System.Diagnostics;

begin
  var SW := TStopwatch.StartNew;
  Write('before ');
  Sleep(1234);
  WriteLn(TInstant.Now.ToString, ' ', SW.ElapsedMilliseconds);
  Sleep(0);
  WriteLn(TInstant.Now.ToString, ' ', SW.ElapsedMilliseconds);
  Sleep(High(Cardinal));
  WriteLn(TInstant.Now.ToString, ' ', SW.ElapsedMilliseconds);
end.
