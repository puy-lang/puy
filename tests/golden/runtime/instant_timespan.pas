program InstantTimeSpan;

// TInstant and TTimeSpan: an instant moved by a time span, and the time span between two
// instants (the nanoseconds below a tick of 100 nanoseconds dropped toward zero).

uses
  System.SysUtils;

var
  A, B: TInstant;
begin
  A := TInstant.Parse('2026-11-20T12:00:00Z');
  B := A + TTimeSpan.FromHours(36.5);
  WriteLn(B.ToString);
  WriteLn((TTimeSpan.FromMinutes(-1) + A).ToString);
  WriteLn((A - TTimeSpan.Create(1, 0, 0, 0)).ToString);
  WriteLn((B - A).ToString, ' ', (A - B).ToString);
  WriteLn((TInstant.FromUnixNanos(250) - TInstant.FromUnixNanos(0)).Ticks, ' ',
    (TInstant.FromUnixNanos(0) - TInstant.FromUnixNanos(250)).Ticks, ' ',
    (TInstant.FromUnixNanos(-199) - TInstant.FromUnixNanos(1)).Ticks, ' ',
    (TInstant.FromUnixNanos(1) - TInstant.FromUnixNanos(-99)).Ticks);
  WriteLn((TInstant.FromUnixNanos(High(Int64)) - TInstant.FromUnixNanos(Low(Int64))).ToString);
  WriteLn((TInstant.FromUnixNanos(Low(Int64)) - TInstant.FromUnixNanos(High(Int64))).Ticks);
  try
    WriteLn((A + TTimeSpan.FromDays(200 * 365)).ToString);
  except
    on E: ERangeError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    WriteLn((A + TTimeSpan.MaxValue).ToString);
  except
    on E: ERangeError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end.
