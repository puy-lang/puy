// puy-test: no-random, absent=unsupported
program RandomNoSourceUnsupported;

// On a target without a source of random numbers, TRandom.Create raises ENotSupportedException
// rather than running from a fixed seed; a TRandom with a seed still works.

uses
  System.SysUtils, Puy.Random;

var
  R: TRandom;
begin
  try
    R := TRandom.Create;
    WriteLn(R.Seed);
  except
    on E: ENotSupportedException do
      WriteLn(E.ClassName, ': ', E.Message, ' ', E.ErrorKind);
  end;
  R := TRandom.Create(0);
  WriteLn(R.NextUInt64);
end.
