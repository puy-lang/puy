program RandomSequence;

// TRandom with a seed gives a sequence fixed by the specification (xoshiro256** seeded by
// SplitMix64; docs/spec/stdlib/Puy.Random/README.md 3), and the ways values are made from it.
// The values were checked against a reference written in Python.

uses
  System.SysUtils, System.Generics.Collections, Puy.Random;

procedure ShowInts(const A: TArray<Integer>);
var
  I: Integer;
begin
  for I := 0 to High(A) do
    Write(A[I], ' ');
  WriteLn;
end;

var
  R, R2: TRandom;
  I: Integer;
  A: TArray<Integer>;
  B: TBytes;
  L: TList<string>;
  S: string;
  Same: Boolean;
  N: Integer;
  D: Double;
begin
  // The first values of the seed 0
  R := TRandom.Create(0);
  for I := 1 to 4 do
    WriteLn(R.NextUInt64);
  WriteLn(R.Seed);

  R := TRandom.Create(42);
  WriteLn('Next: ', R.Next);
  WriteLn('NextInt64: ', R.NextInt64);
  WriteLn('Next(10): ', R.Next(10));
  WriteLn('Next(-5, 5): ', R.Next(-5, 5));
  WriteLn('NextInt64(1000000000000): ', R.NextInt64(1000000000000));
  WriteLn('NextInt64(Low, High): ', R.NextInt64(Low(Int64), High(Int64)));
  WriteLn('Next(Low, High): ', R.Next(Low(Integer), High(Integer)));
  WriteLn('Next(1): ', R.Next(1));
  WriteLn('NextDouble: ', R.NextDouble:0:17);
  WriteLn('NextGaussian: ', R.NextGaussian:0:17);
  WriteLn('NextGaussian(10, 2): ', R.NextGaussian(10, 2):0:17);

  // Bytes, eight to a value from the lowest; the rest of the last value is dropped
  B := R.NextBytes(11);
  for I := 0 to High(B) do
    Write(B[I], ' ');
  WriteLn;
  SetLength(B, 3);
  R.NextBytes(B);
  for I := 0 to High(B) do
    Write(B[I], ' ');
  WriteLn;
  WriteLn(Length(R.NextBytes(0)));

  // Shuffling and choosing
  A := [1, 2, 3, 4, 5, 6, 7, 8];
  R.Shuffle<Integer>(A);
  ShowInts(A);
  L := TList<string>.Create;
  L.AddRange(['a', 'b', 'c', 'd', 'e']);
  R.Shuffle<string>(L);
  for S in L do
    Write(S);
  WriteLn;
  L.Free;
  WriteLn(R.Choice<string>(['x', 'y', 'z']));
  A := [7];
  R.Shuffle<Integer>(A);
  ShowInts(A);

  // The same seed gives the same sequence; the negative seeds are their 64 bits
  R := TRandom.Create(-1);
  R2 := TRandom.Create(-1);
  Same := True;
  for I := 1 to 100 do
    if R.NextUInt64 <> R2.NextUInt64 then
      Same := False;
  WriteLn('same: ', Same, ' ', R.Seed);

  // Next(N) and NextInt64(N) depend only on the width: the same values, the same steps
  R := TRandom.Create(7);
  R2 := TRandom.Create(7);
  Same := True;
  for I := 1 to 100 do
    if (R.Next(I) <> R2.NextInt64(I)) or (R.Next(-I, I) <> R2.NextInt64(-I, I)) then
      Same := False;
  WriteLn('same widths: ', Same);

  // Every value stays in its range
  R := TRandom.Create(1);
  Same := True;
  for I := 1 to 1000 do
  begin
    // Both bounds are checked on the same drawn value
    N := R.Next(3);
    if (N < 0) or (N >= 3) then
      Same := False;
    N := R.Next(-2, 2);
    if (N < -2) or (N >= 2) then
      Same := False;
    D := R.NextDouble;
    if (D < 0) or (D >= 1) then
      Same := False;
  end;
  WriteLn('in range: ', Same);
end.
