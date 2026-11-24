// puy-test: clock=0, mono-step=150
program StopwatchTicks;

// The stopwatch keeps nanoseconds and cuts them to ticks of 100 ns (and to milliseconds) only
// when read: two intervals of 150 ns make 3 ticks, not 1 + 1. A loop that waits for an elapsed
// time ends after a deterministic number of readings on the test clock (150 ns per read).

uses
  System.Diagnostics;

begin
  var SW := TStopwatch.Create;
  SW.Start;
  SW.Stop;
  WriteLn(SW.ElapsedTicks);
  SW.Start;
  SW.Stop;
  WriteLn(SW.ElapsedTicks, ' ', SW.Elapsed.Ticks);
  var N := 0;
  SW := TStopwatch.StartNew;
  while SW.ElapsedMilliseconds < 1 do
    Inc(N);
  WriteLn(N, ' ', SW.ElapsedTicks);
end.
