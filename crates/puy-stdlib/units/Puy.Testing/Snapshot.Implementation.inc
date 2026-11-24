// ===== AssertSnapshot =====

// Both versions call Snapshot (Puy.Testing.Native) themselves: it takes the position of the
// snapshot from the frame of their caller.

procedure AssertSnapshot(const Actual: string);
var
  Failure: string;
begin
  Failure := Snapshot(Actual, '', False);
  if Failure <> '' then
    raise EAssertionFailed.Create(Failure);
end;

procedure AssertSnapshot(const Actual: string; const Name: string);
var
  Failure: string;
begin
  Failure := Snapshot(Actual, Name, True);
  if Failure <> '' then
    raise EAssertionFailed.Create(Failure);
end;
