// The virtual clock under `puy test` (docs/spec/stdlib/Puy.Testing/README.md chapter 6): a frozen
// test sleeps an hour without waiting, the next test starts on the real clock again (each test
// has an isolate of its own), and the time limit stays on real time even on a frozen clock.
unit Clock_test;

interface

implementation

uses
  System.SysUtils, System.Diagnostics, Puy.Testing;

[Test]
procedure SleepsAnHour;
begin
  TTestClock.Freeze(TInstant.Parse('2026-11-21T09:00:00Z'));
  var SW := TStopwatch.StartNew;
  Sleep(60 * 60 * 1000);
  Assert(TInstant.Now.ToString = '2026-11-21T10:00:00Z');
  Assert(SW.ElapsedMilliseconds = 3600000);
end;

[Test]
procedure StartsOnTheRealClock;
begin
  AssertRaises<EInvalidOperation>(procedure begin TTestClock.Advance(1) end);
end;

[Test]
procedure FrozenForever;
var
  N: Integer;
begin
  TTestClock.Freeze;
  N := 0;
  while N >= 0 do
    N := (N + 1) mod 1000;
end;

end.
