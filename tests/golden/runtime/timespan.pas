program TimeSpanUse;

// TTimeSpan: made from parts, units and text, its parts and totals, the arithmetic and the
// comparisons, text, and the errors. System.TimeSpan names the same type as System.

uses
  System.SysUtils, System.TimeSpan;

procedure Show(const Name: string; const T: TTimeSpan);
begin
  WriteLn(Name, ' ', T.ToString, ' ticks ', T.Ticks, ' parts ', T.Days, ' ', T.Hours, ' ',
    T.Minutes, ' ', T.Seconds, ' ', T.Milliseconds);
end;

procedure Parse(const S: string);
var
  T: TTimeSpan;
begin
  if TTimeSpan.TryParse(S, T) then
    WriteLn('''', S, ''' ', T.ToString)
  else
    WriteLn('''', S, ''' fails');
end;

procedure Fails(const Name: string; const P: TProc);
begin
  try
    P();
    WriteLn(Name, ': no exception');
  except
    on E: Exception do
      WriteLn(Name, ': ', E.ClassName, ': ', E.Message);
  end;
end;

var
  A, B: TTimeSpan;
  C: System.TTimeSpan;
begin
  Show('Create ticks', TTimeSpan.Create(12345678901));
  Show('Create hms', TTimeSpan.Create(1, 2, 3));
  Show('Create dhms', TTimeSpan.Create(2, 3, 4, 5));
  Show('Create dhmsm', TTimeSpan.Create(1, 2, 3, 4, 5));
  Show('Create carry', TTimeSpan.Create(0, 25, 61, 61, 1001));
  Show('Create negative', TTimeSpan.Create(0, -1, 30, 0));
  Show('FromDays', TTimeSpan.FromDays(1.5));
  Show('FromHours', TTimeSpan.FromHours(-2.25));
  Show('FromMinutes', TTimeSpan.FromMinutes(90.5));
  Show('FromSeconds', TTimeSpan.FromSeconds(0.0000001));
  Show('FromSeconds half tick', TTimeSpan.FromSeconds(0.00000025));
  Show('FromMilliseconds', TTimeSpan.FromMilliseconds(1.23456));
  Show('FromTicks', TTimeSpan.FromTicks(-1));
  Show('Zero', TTimeSpan.Zero);
  Show('MinValue', TTimeSpan.MinValue);
  Show('MaxValue', TTimeSpan.MaxValue);
  WriteLn(TTimeSpan.TicksPerMillisecond, ' ', TTimeSpan.TicksPerSecond, ' ',
    TTimeSpan.TicksPerMinute, ' ', TTimeSpan.TicksPerHour, ' ', TTimeSpan.TicksPerDay);

  A := TTimeSpan.Create(1, 12, 0, 0);
  B := TTimeSpan.FromMinutes(-90);
  WriteLn(A.TotalDays:0:4, ' ', A.TotalHours:0:4, ' ', A.TotalMinutes:0:1, ' ', A.TotalSeconds:0:1,
    ' ', A.TotalMilliseconds:0:1, ' ', B.TotalHours:0:4);
  Show('A + B', A + B);
  Show('A - B', A - B);
  Show('-A', -A);
  Show('+B', +B);
  Show('Add', A.Add(B));
  Show('Subtract', A.Subtract(B));
  Show('Duration', B.Duration);
  Show('Negate', B.Negate);
  WriteLn(A = B, ' ', A <> B, ' ', A < B, ' ', A <= A, ' ', A > B, ' ', B >= A, ' ',
    A.CompareTo(B), ' ', B.CompareTo(A), ' ', A.CompareTo(A));
  WriteLn(KeyEquals(A, TTimeSpan.FromHours(36)), ' ', KeyHash(A) = KeyHash(TTimeSpan.FromHours(36)),
    ' ', A.GetHashCode = TTimeSpan.FromDays(1.5).GetHashCode);
  C := A;
  WriteLn(C.ToString, ' ', System.TTimeSpan.Zero = TTimeSpan.Zero);

  Parse('1:02');
  Parse('01:02:03');
  Parse('1.02:03:04.5');
  Parse('-0:00:00.0000001');
  Parse('  7  ');
  Parse('-3');
  Parse('10675199.02:48:05.4775807');
  Parse('-10675199.02:48:05.4775808');
  Parse('10675199.02:48:05.4775808');
  Parse('24:00');
  Parse('1:60');
  Parse('1:2');
  Parse('123:00');
  Parse('1:00:00.12345678');
  Parse('1.:00');
  Parse('');
  Parse('+1:00');
  WriteLn(TTimeSpan.Parse(TTimeSpan.MinValue.ToString) = TTimeSpan.MinValue);

  Fails('Parse', procedure begin TTimeSpan.Parse('x') end);
  Fails('Create', procedure begin TTimeSpan.Create(MaxInt, 0, 0, 0) end);
  Fails('FromDays', procedure begin TTimeSpan.FromDays(1e7 * 2) end);
  Fails('FromSeconds NaN', procedure begin TTimeSpan.FromSeconds(0.0 / 0.0) end);
  Fails('Add', procedure begin TTimeSpan.MaxValue.Add(TTimeSpan.FromTicks(1)) end);
  Fails('Subtract', procedure begin TTimeSpan.MinValue.Subtract(TTimeSpan.FromTicks(1)) end);
  Fails('Negate', procedure begin TTimeSpan.MinValue.Negate end);
  Fails('Duration', procedure begin TTimeSpan.MinValue.Duration end);
end.
