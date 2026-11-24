// puy-test: no-clock
program StopwatchNoClock;

// Without a clock, what does not read it works (Create, the properties of a stopped stopwatch,
// Reset, Frequency) and Start, StartNew and GetTimeStamp raise ENotSupportedException.

uses
  System.SysUtils, System.Diagnostics;

begin
  var SW := TStopwatch.Create;
  SW.Reset;
  WriteLn(SW.IsRunning, ' ', SW.ElapsedTicks, ' ', SW.Elapsed.ToString, ' ', TStopwatch.Frequency);
  try
    SW.Start;
  except
    on E: ENotSupportedException do
      WriteLn(E.ClassName, ': ', E.Message, ' ', E.ErrorKind);
  end;
  WriteLn(SW.IsRunning);
  try
    SW := TStopwatch.StartNew;
    WriteLn(SW.IsRunning);
  except
    on E: ENotSupportedException do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    WriteLn(TStopwatch.GetTimeStamp);
  except
    on E: ENotSupportedException do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end.
