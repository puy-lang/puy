unit Puy.Random;

// TRandom, a generator of pseudo-random numbers with a state of its own
// (docs/spec/stdlib/Puy.Random/README.md). The algorithm is xoshiro256**, its state made from the
// 64-bit seed by SplitMix64; the ways numbers are made from the sequence (ranges, Double,
// Gaussian, bytes, shuffling) are part of the specification and do not change, so that a seed
// gives the same values on every target and version.

interface

uses
  System.SysUtils, System.Generics.Collections;

type
  TRandom = class(TRefObject)
  private
    FSeed: Int64;
    FS0, FS1, FS2, FS3: UInt64;
    procedure Init(Seed: Int64);
    function Below(Bound: UInt64): UInt64;
  public
    constructor Create; overload;
    constructor Create(Seed: Int64); overload;
    function NextUInt64: UInt64;
    function Next: Integer; overload;
    function Next(MaxValue: Integer): Integer; overload;
    function Next(MinValue, MaxValue: Integer): Integer; overload;
    function NextInt64: Int64; overload;
    function NextInt64(MaxValue: Int64): Int64; overload;
    function NextInt64(MinValue, MaxValue: Int64): Int64; overload;
    function NextDouble: Double;
    function NextGaussian: Double; overload;
    function NextGaussian(Mean, StdDev: Double): Double; overload;
    procedure NextBytes(var Buffer: TBytes); overload;
    function NextBytes(Count: Integer): TBytes; overload;
    procedure Shuffle<T>(var Values: array of T); overload;
    procedure Shuffle<T>(List: TList<T>); overload;
    function Choice<T>(const Values: array of T): T;
    property Seed: Int64 read FSeed;
  end;

implementation

uses
  // The Rust part of the unit (crates/puy-stdlib-ext; docs/design/stdlib.md 3.1)
  Puy.Random.Native;

// The generator works in wrapping 64-bit arithmetic: the overflow and range checks are off for
// the whole unit (a seed's bits and the sums of a range are reinterpreted, not checked).
{$Q-}
{$R-}

function RotL(X: UInt64; K: Integer): UInt64; inline;
begin
  Result := (X shl K) or (X shr (64 - K));
end;

{ TRandom }

constructor TRandom.Create;
begin
  inherited Create;
  // 8 bytes of the host's source of random numbers (an exception when there is none)
  Init(OsSeed);
end;

constructor TRandom.Create(Seed: Int64);
begin
  inherited Create;
  Init(Seed);
end;

// The state is four outputs of SplitMix64 started at the seed's 64 bits.
procedure TRandom.Init(Seed: Int64);
var
  X, Z: UInt64;
  I: Integer;
begin
  FSeed := Seed;
  X := UInt64(Seed);
  for I := 0 to 3 do
  begin
    X := X + UInt64($9E3779B97F4A7C15);
    Z := X;
    Z := (Z xor (Z shr 30)) * UInt64($BF58476D1CE4E5B9);
    Z := (Z xor (Z shr 27)) * UInt64($94D049BB133111EB);
    Z := Z xor (Z shr 31);
    case I of
      0: FS0 := Z;
      1: FS1 := Z;
      2: FS2 := Z;
    else
      FS3 := Z;
    end;
  end;
end;

function TRandom.NextUInt64: UInt64;
var
  T: UInt64;
begin
  Result := RotL(FS1 * 5, 7) * 9;
  T := FS1 shl 17;
  FS2 := FS2 xor FS0;
  FS3 := FS3 xor FS1;
  FS1 := FS1 xor FS2;
  FS0 := FS0 xor FS3;
  FS2 := FS2 xor T;
  FS3 := RotL(FS3, 45);
end;

// A value from 0 to Bound - 1 without bias (Bound >= 1): the low bits of each value under the
// smallest mask of all ones that covers Bound - 1, drawn again while they are Bound or more.
// Each draw takes one value of the sequence.
function TRandom.Below(Bound: UInt64): UInt64;
var
  Mask: UInt64;
begin
  Mask := Bound - 1;
  Mask := Mask or (Mask shr 1);
  Mask := Mask or (Mask shr 2);
  Mask := Mask or (Mask shr 4);
  Mask := Mask or (Mask shr 8);
  Mask := Mask or (Mask shr 16);
  Mask := Mask or (Mask shr 32);
  repeat
    Result := NextUInt64 and Mask;
  until Result < Bound;
end;

function TRandom.Next: Integer;
begin
  Result := Integer(NextUInt64 shr 33);
end;

function TRandom.Next(MaxValue: Integer): Integer;
begin
  if MaxValue <= 0 then
    raise EArgumentException.CreateFmt('MaxValue must be positive, found %d', [MaxValue]);
  Result := Integer(Below(UInt64(MaxValue)));
end;

function TRandom.Next(MinValue, MaxValue: Integer): Integer;
begin
  if MaxValue <= MinValue then
    raise EArgumentException.CreateFmt('MaxValue must be greater than MinValue, found %d and %d',
      [MinValue, MaxValue]);
  Result := Integer(Int64(MinValue) + Int64(Below(UInt64(Int64(MaxValue) - Int64(MinValue)))));
end;

function TRandom.NextInt64: Int64;
begin
  Result := Int64(NextUInt64 shr 1);
end;

function TRandom.NextInt64(MaxValue: Int64): Int64;
begin
  if MaxValue <= 0 then
    raise EArgumentException.CreateFmt('MaxValue must be positive, found %d', [MaxValue]);
  Result := Int64(Below(UInt64(MaxValue)));
end;

function TRandom.NextInt64(MinValue, MaxValue: Int64): Int64;
begin
  if MaxValue <= MinValue then
    raise EArgumentException.CreateFmt('MaxValue must be greater than MinValue, found %d and %d',
      [MinValue, MaxValue]);
  // The width of the range is at most 2^64 - 1, which fits in UInt64; the sum wraps back into
  // the range
  Result := Int64(UInt64(MinValue) + Below(UInt64(MaxValue) - UInt64(MinValue)));
end;

// The top 53 bits as a fraction of 2^53: from 0 up to 1 - 2^-53.
function TRandom.NextDouble: Double;
begin
  Result := (NextUInt64 shr 11) / 9007199254740992.0;
end;

// One value of the Box-Muller transform for two draws: U1 in (0, 1] and U2 in [0, 1).
function TRandom.NextGaussian: Double;
var
  U1, U2: Double;
begin
  U1 := 1.0 - NextDouble;
  U2 := NextDouble;
  Result := Sqrt(-2.0 * Ln(U1)) * Cos(2.0 * Pi * U2);
end;

function TRandom.NextGaussian(Mean, StdDev: Double): Double;
begin
  Result := Mean + StdDev * NextGaussian;
end;

// The bytes of each value from the lowest, eight to a value; what the last value has over is
// dropped.
procedure TRandom.NextBytes(var Buffer: TBytes);
var
  I, K: Integer;
  V: UInt64;
begin
  I := 0;
  while I < Length(Buffer) do
  begin
    V := NextUInt64;
    K := 0;
    while (K < 8) and (I < Length(Buffer)) do
    begin
      Buffer[I] := Byte(V shr (8 * K));
      Inc(I);
      Inc(K);
    end;
  end;
end;

function TRandom.NextBytes(Count: Integer): TBytes;
begin
  if Count < 0 then
    raise EArgumentException.CreateFmt('Count must not be negative, found %d', [Count]);
  SetLength(Result, Count);
  NextBytes(Result);
end;

// Fisher-Yates from the end: the element at I (from the last down to 1) changes places with the
// one at Next(I + 1).
procedure TRandom.Shuffle<T>(var Values: array of T);
var
  I, J: Integer;
  Tmp: T;
begin
  for I := High(Values) downto 1 do
  begin
    J := Integer(Below(UInt64(I + 1)));
    Tmp := Values[I];
    Values[I] := Values[J];
    Values[J] := Tmp;
  end;
end;

procedure TRandom.Shuffle<T>(List: TList<T>);
var
  I: Integer;
begin
  for I := List.Count - 1 downto 1 do
    List.Exchange(I, Integer(Below(UInt64(I + 1))));
end;

function TRandom.Choice<T>(const Values: array of T): T;
begin
  if Length(Values) = 0 then
    raise EArgumentException.Create('Values must not be empty');
  Result := Values[Below(UInt64(Length(Values)))];
end;

end.
