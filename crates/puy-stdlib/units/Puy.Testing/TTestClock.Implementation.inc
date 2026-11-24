// ===== The virtual clock (docs/spec/stdlib/Puy.Testing/README.md chapter 6) =====

const
  // What ClockControl does, and its statuses (docs/spec/bytecode.md 2.22; the numbers of
  // bytecode::CLOCK_* in puy-core: keep the two in step)
  TcFreeze = 0;
  TcFreezeAt = 1;
  TcAdvance = 2;
  TcSetNow = 3;
  TcNotFrozen = 1;
  TcOutOfRange = 2;

// Raises the exception of a status of ClockControl; What names the call.
procedure TcCheck(Status: Integer; const What: string);
begin
  if Status = TcNotFrozen then
    raise EInvalidOperation.Create(What +
      ' needs a frozen virtual clock: call TTestClock.Freeze first');
  if Status = TcOutOfRange then
    raise ERangeError.Create(What + ' would move the virtual clock past its range');
end;

class procedure TTestClock.Freeze;
begin
  ClockControl(TcFreeze, 0);
end;

class procedure TTestClock.Freeze(const At: TInstant);
begin
  ClockControl(TcFreezeAt, At.ToUnixNanos);
end;

class procedure TTestClock.Advance(Milliseconds: Int64);
begin
  var What := 'TTestClock.Advance(' + IntToStr(Milliseconds) + ')';
  if Milliseconds < 0 then
    raise EArgumentException.Create(What + ': the time is negative');
  if Milliseconds > High(Int64) div 1000000 then
    TcCheck(TcOutOfRange, What);
  TcCheck(ClockControl(TcAdvance, Milliseconds * 1000000), What);
end;

class procedure TTestClock.SetNow(const At: TInstant);
begin
  TcCheck(ClockControl(TcSetNow, At.ToUnixNanos), 'TTestClock.SetNow');
end;
