// puy-test: random=-6148914691236517206
program RandomSeed;

// TRandom.Create takes its seed from 8 bytes of the host's source of random numbers
// (little-endian; the runner's source repeats the bytes of the value given), and gives the same
// sequence as TRandom.Create with that seed.

uses
  Puy.Random;

var
  R, R2: TRandom;
  I: Integer;
  Same: Boolean;
begin
  R := TRandom.Create;
  WriteLn(R.Seed);
  R2 := TRandom.Create(R.Seed);
  Same := True;
  for I := 1 to 10 do
    if R.NextUInt64 <> R2.NextUInt64 then
      Same := False;
  WriteLn(Same);
end.
