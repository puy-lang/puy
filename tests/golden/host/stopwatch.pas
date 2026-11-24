// puy-test: clock=0, mono=5000000000, mono-step=1500000
program Stopwatch;

// TStopwatch reads the host's monotonic clock once in Start, Stop, Restart, GetTimeStamp and each
// reading of an elapsed property while running; the test clock moves 1.5 ms at each read, so
// the elapsed times are exact. Start on a running stopwatch and Stop on a stopped one do not
// read the clock. A copy of the record goes on on its own.

uses
  System.Diagnostics;

var
  Zero: TStopwatch;

begin
  WriteLn('zero: ', Zero.IsRunning, ' ', Zero.ElapsedTicks, ' ', Zero.ElapsedMilliseconds);
  var SW := TStopwatch.Create;
  WriteLn('create: ', SW.IsRunning, ' ', SW.ElapsedTicks, ' ', SW.Elapsed.ToString);
  SW.Start;
  WriteLn('running: ', SW.IsRunning);
  WriteLn('ticks: ', SW.ElapsedTicks);
  WriteLn('ms: ', SW.ElapsedMilliseconds);
  SW.Stop;
  WriteLn('stopped: ', SW.IsRunning, ' ', SW.ElapsedTicks, ' ', SW.Elapsed.ToString, ' ',
    SW.Elapsed.TotalMilliseconds:0:1);
  SW.Stop;
  SW.Start;
  SW.Start;
  SW.Stop;
  WriteLn('again: ', SW.ElapsedTicks, ' ', SW.ElapsedMilliseconds);
  SW.Reset;
  WriteLn('reset: ', SW.IsRunning, ' ', SW.ElapsedTicks);
  SW.Restart;
  WriteLn('restart: ', SW.IsRunning, ' ', SW.ElapsedTicks);
  var Copied := SW;
  SW.Stop;
  WriteLn('original: ', SW.ElapsedTicks, ', copy: ', Copied.IsRunning, ' ', Copied.ElapsedTicks);
  SW.Restart;
  SW.Restart;
  WriteLn('restart twice: ', SW.ElapsedTicks);
  var SW3 := TStopwatch.StartNew;
  WriteLn('start new: ', SW3.IsRunning, ' ', SW3.ElapsedTicks);
  WriteLn('frequency: ', TStopwatch.Frequency, ' ', TStopwatch.IsHighResolution);
  var T1 := TStopwatch.GetTimeStamp;
  var T2 := TStopwatch.GetTimeStamp;
  WriteLn('time stamps: ', T1, ' ', T2 - T1);
end.
