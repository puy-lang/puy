program InstantErrors;

// TInstant raises EConvertError for text that is not RFC 3339 (the value cut in the message as
// the built-in conversions cut it) and ERangeError for a value out of its range.

procedure Check(const What: string; const P: TProc);
begin
  try
    P();
    WriteLn(What, ': no exception');
  except
    on E: EConvertError do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message, ' [', Length(E.Value), ']');
    on E: Exception do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

begin
  Check('parse', procedure begin TInstant.Parse('2026-13-01T00:00:00Z') end);
  Check('long', procedure
    begin
      var S: string := 'x';
      while Length(S) < 150 do
        S := S + 'x';
      TInstant.Parse(S);
    end);
  Check('seconds', procedure begin TInstant.FromUnixSeconds(9300000000) end);
  Check('millis', procedure begin TInstant.FromUnixMillis(-9300000000000) end);
  Check('add', procedure begin TInstant.FromUnixNanos(High(Int64)).AddNanos(1) end);
  Check('add seconds', procedure begin TInstant.FromUnixSeconds(0).AddSeconds(9300000000) end);
  Check('since', procedure
    begin
      TInstant.FromUnixNanos(High(Int64)).NanosSince(TInstant.FromUnixNanos(-1))
    end);
  Check('date time', procedure begin TInstant.FromDateTime(1.0e8) end);
  Check('date time year 1500', procedure begin TInstant.FromDateTime(-146000) end);
end.
