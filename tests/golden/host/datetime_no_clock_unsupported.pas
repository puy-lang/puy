// puy-test: no-clock, absent=unsupported
program DateTimeNoClockUnsupported;

// A target without a clock (the browser until the Playground gives one) says so in the message
// and the kind.

begin
  try
    WriteLn(TInstant.Now.ToString);
  except
    on E: ENotSupportedException do
      WriteLn(E.Message, ' ', E.ErrorKind);
  end;
end.
