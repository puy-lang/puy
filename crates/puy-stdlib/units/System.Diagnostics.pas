unit System.Diagnostics;

// TStopwatch (docs/spec/stdlib/System.Diagnostics/README.md): elapsed time on the monotonic clock
// (requirements §8.8). Written from the specification only, not from any other implementation.
// Every reading of the clock goes through ReadClock, so that the virtual clock of the tests,
// which the built-in ClockMono follows (Puy.Testing), needs no change here. The stopwatch keeps
// nanoseconds and gives ticks of 100 nanoseconds, the unit of TTimeSpan (Frequency).

interface

type
  // Measures elapsed time. A record: copying it copies its state. The default value is a
  // stopped stopwatch with nothing elapsed, as Create gives.
  TStopwatch = record
  private
    FElapsed: Int64;
    FStart: Int64;
    FRunning: Boolean;
    class function ReadClock: Int64; static;
    class function GetFrequency: Int64; static;
    class function GetIsHighResolution: Boolean; static;
    function ElapsedNanos: Int64;
    function GetElapsed: TTimeSpan;
    function GetElapsedMilliseconds: Int64;
    function GetElapsedTicks: Int64;
  public
    class function Create: TStopwatch; static;
    class function StartNew: TStopwatch; static;
    class function GetTimeStamp: Int64; static;

    procedure Start;
    procedure Stop;
    procedure Reset;
    procedure Restart;

    property Elapsed: TTimeSpan read GetElapsed;
    property ElapsedMilliseconds: Int64 read GetElapsedMilliseconds;
    property ElapsedTicks: Int64 read GetElapsedTicks;
    property IsRunning: Boolean read FRunning;

    class property Frequency: Int64 read GetFrequency;
    class property IsHighResolution: Boolean read GetIsHighResolution;
  end;

implementation

const
  // Nanoseconds in a tick of the stopwatch (and of TTimeSpan)
  SwNanosPerTick = 100;
  SwNanosPerMilli = 1000000;

// The one place that reads the time: the host's monotonic clock in nanoseconds
class function TStopwatch.ReadClock: Int64;
begin
  Result := ClockMono;
end;

class function TStopwatch.GetFrequency: Int64;
begin
  Result := TTimeSpan.TicksPerSecond;
end;

class function TStopwatch.GetIsHighResolution: Boolean;
begin
  Result := True;
end;

class function TStopwatch.Create: TStopwatch;
begin
  Result := Default(TStopwatch);
end;

class function TStopwatch.StartNew: TStopwatch;
begin
  Result := Create;
  Result.Start;
end;

class function TStopwatch.GetTimeStamp: Int64;
begin
  Result := ReadClock div SwNanosPerTick;
end;

procedure TStopwatch.Start;
begin
  if not FRunning then
  begin
    FStart := ReadClock;
    FRunning := True;
  end;
end;

procedure TStopwatch.Stop;
begin
  if FRunning then
  begin
    FElapsed := FElapsed + (ReadClock - FStart);
    FRunning := False;
  end;
end;

procedure TStopwatch.Reset;
begin
  FElapsed := 0;
  FRunning := False;
end;

procedure TStopwatch.Restart;
begin
  // The clock first, so that a failure leaves the stopwatch as it was
  var Stamp := ReadClock;
  FElapsed := 0;
  FStart := Stamp;
  FRunning := True;
end;

// What has elapsed: the stopped intervals and, while running, the one going on
function TStopwatch.ElapsedNanos: Int64;
begin
  Result := FElapsed;
  if FRunning then
    Result := Result + (ReadClock - FStart);
end;

function TStopwatch.GetElapsed: TTimeSpan;
begin
  Result := TTimeSpan.FromTicks(ElapsedNanos div SwNanosPerTick);
end;

function TStopwatch.GetElapsedMilliseconds: Int64;
begin
  Result := ElapsedNanos div SwNanosPerMilli;
end;

function TStopwatch.GetElapsedTicks: Int64;
begin
  Result := ElapsedNanos div SwNanosPerTick;
end;

end.
