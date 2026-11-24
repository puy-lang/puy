// A timed-out test has no exit status and the next selected test still runs.
unit Loop_test;

interface

implementation

[Test]
procedure Spins;
begin
  while True do
    begin
    end;
end;

[Test]
procedure Next;
begin
end;

end.
