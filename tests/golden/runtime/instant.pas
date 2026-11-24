program Instant;

// TInstant (docs/spec/stdlib/System/TInstant.md): needs no `uses`; Unix times, RFC 3339 with the
// fewest fraction digits that write the value exactly, offsets, comparison, the UTC TDateTime.

procedure TryParse(const S: string);
var
  T: TInstant;
begin
  if TInstant.TryParse(S, T) then
    WriteLn(S, ' -> ', T.ToString)
  else
    WriteLn(S, ' -> invalid');
end;

begin
  var T := TInstant.FromUnixSeconds(1793502245);
  WriteLn(T.ToString, ' ', T.ToUnixSeconds, ' ', T.ToUnixMillis, ' ', T.ToUnixNanos);
  WriteLn(T.AddMillis(123).ToString);
  WriteLn(T.AddNanos(123456000).ToString);
  WriteLn(T.AddNanos(1).ToString);
  WriteLn(T.AddSeconds(-1793502245).ToString);
  WriteLn(TInstant.FromUnixMillis(-1).ToString, ' ', TInstant.FromUnixMillis(-1).ToUnixSeconds);
  WriteLn(TInstant.FromUnixNanos(Low(Int64)).ToString);
  WriteLn(TInstant.FromUnixNanos(High(Int64)).ToString);
  WriteLn(TInstant.FromUnixNanos(951782400000000000).ToString);

  TryParse('2026-11-01T03:04:05Z');
  TryParse('2026-11-01t03:04:05.1z');
  TryParse('2026-11-01 12:04:05.123456789123+09:00');
  TryParse('2026-01-01T00:30:00-01:00');
  TryParse('2024-02-29T00:00:00Z');
  TryParse('2026-02-29T00:00:00Z');
  TryParse('2026-11-01T24:00:00Z');
  TryParse('2026-11-01T03:04:60Z');
  TryParse('2026-11-01T03:04:05');
  TryParse('2026-11-01T03:04:05.Z');
  TryParse('2026-11-01T03:04:05+0900');
  TryParse('2026-11-01T03:04:05Z ');
  TryParse('2263-01-01T00:00:00Z');
  TryParse('1677-09-21T00:12:43.145224192Z');
  TryParse('1677-09-21T00:12:43.145224191Z');
  TryParse('2262-04-11T23:47:16.854775808Z');
  TryParse('');

  var A := TInstant.Parse('2026-11-01T12:00:00+09:00');
  var B := TInstant.Parse('2026-11-01T03:00:00Z');
  WriteLn(A = B, ' ', A <> B, ' ', A < B.AddNanos(1), ' ', A <= B, ' ', A > B, ' ', A >= B);
  WriteLn(A.CompareTo(B.AddSeconds(1)), ' ', A.CompareTo(B), ' ', B.AddSeconds(1).CompareTo(A));
  WriteLn(B.AddSeconds(90).NanosSince(A), ' ', A.NanosSince(B.AddSeconds(90)));
  WriteLn(KeyEquals(A, B), ' ', A.GetHashCode = B.GetHashCode, ' ', KeyCompare(A, B.AddNanos(1)));

  // The UTC TDateTime, to the millisecond
  WriteLn(TInstant.FromDateTime(A.ToDateTime).ToString);
  WriteLn(TInstant.FromDateTime(25569.5).ToString, ' ', TInstant.FromDateTime(-1.25).ToString);
  WriteLn(TInstant.Parse('1899-12-29T06:00:00Z').ToDateTime = -1.25);
  var S: System.TInstant := B;
  WriteLn(S.ToString);
end.
