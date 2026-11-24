program MathBasic;

// The minimum and the maximum, ranges, signs, Floor / Ceil, RoundTo / SimpleRoundTo, comparisons
// of real numbers, and the sums and statistics of arrays (System.Math).

uses
  System.SysUtils, System.Math;

type
  TDoubles = array of Double;
  TInts = array of Integer;

function Doubles(const A: array of Double): TDoubles;
var
  I: Integer;
begin
  SetLength(Result, Length(A));
  for I := 0 to High(A) do
    Result[I] := A[I];
end;

var
  I: Integer;
  Data: TDoubles;
  Ints: TInts;
begin
  WriteLn(Min(3, 7), ' ', Max(3, 7), ' ', Min(-2, -5), ' ', Max(-2, -5));
  WriteLn(Min(Int64(10000000000), Int64(3)), ' ', Max(Int64(10000000000), Int64(3)));
  WriteLn(Min(1.5, 2.5):0:2, ' ', Max(1.5, 2.5):0:2, ' ', Min(2, 0.5):0:2);

  WriteLn(InRange(5, 1, 10), ' ', InRange(0, 1, 10), ' ', InRange(10, 1, 10));
  WriteLn(InRange(2.5, 1.0, 2.0), ' ', InRange(1.0, 1.0, 2.0));
  WriteLn(EnsureRange(15, 1, 10), ' ', EnsureRange(-3, 1, 10), ' ', EnsureRange(4, 1, 10));
  WriteLn(EnsureRange(2.5, 0.0, 1.0):0:2, ' ', EnsureRange(-1.5, 0.0, 1.0):0:2);

  WriteLn(Sign(-7), ' ', Sign(0), ' ', Sign(42), ' ', Sign(Int64(-10000000000)));
  WriteLn(Sign(-0.5), ' ', Sign(0.0), ' ', Sign(2.5));

  WriteLn(Floor(2.7), ' ', Floor(-2.7), ' ', Floor(-3.0), ' ', Floor(0.0));
  WriteLn(Ceil(2.2), ' ', Ceil(-2.7), ' ', Ceil(3.0), ' ', Ceil(-0.5));

  WriteLn(RoundTo(1234567, 3):0:1, ' ', RoundTo(1234.5678, -2):0:4, ' ', RoundTo(2.5, 0):0:1,
    ' ', RoundTo(3.5, 0):0:1, ' ', RoundTo(-2.5, 0):0:1);
  WriteLn(RoundTo(0.125, -2):0:4, ' ', RoundTo(0.375, -2):0:4, ' ', RoundTo(2500, 3):0:1,
    ' ', RoundTo(3500, 3):0:1);
  WriteLn(SimpleRoundTo(1234.5678):0:4, ' ', SimpleRoundTo(1234.5678, 2):0:1,
    ' ', SimpleRoundTo(2.5, 0):0:1, ' ', SimpleRoundTo(0.125, -2):0:4);

  WriteLn(SameValue(0.1 + 0.2, 0.3), ' ', SameValue(1.0, 1.001), ' ', SameValue(1.0, 1.001, 0.01));
  WriteLn(SameValue(0.0, 1E-20), ' ', SameValue(1000000.0, 1000000.01));
  WriteLn(IsZero(1E-20), ' ', IsZero(0.001), ' ', IsZero(0.001, 0.01));
  WriteLn(CompareValue(1, 2), ' ', CompareValue(2, 2), ' ', CompareValue(3, 2));
  WriteLn(CompareValue(1.5, 2.5), ' ', CompareValue(0.1 + 0.2, 0.3), ' ', CompareValue(3.5, 2.5));
  WriteLn(IfThen(True, 1, 2), ' ', IfThen(False, 1, 2), ' ', IfThen(True, 1.5, 2.5):0:1);

  // Arrays in variables: FPC has Single, Double and Extended versions of these functions
  Data := Doubles([1.0, 2.0, 3.5]);
  SetLength(Ints, 4);
  for I := 0 to 3 do
    Ints[I] := I + 1;
  WriteLn(Sum(Data):0:2, ' ', SumInt(Ints), ' ', SumOfSquares(Data):0:2);
  Data := Doubles([3.0, -1.5, 2.0]);
  WriteLn(MinValue(Data):0:2, ' ', MaxValue(Data):0:2, ' ', Mean(Data):0:6);
  Ints[0] := -3;
  Ints[3] := 9;
  WriteLn(MinIntValue(Ints), ' ', MaxIntValue(Ints));
  Data := Doubles([2.0, 4, 4, 4, 5, 5, 7, 9]);
  WriteLn(Variance(Data):0:6, ' ', StdDev(Data):0:6);
  WriteLn(PopnVariance(Data):0:6, ' ', PopnStdDev(Data):0:6);
  WriteLn(Norm(Doubles([3.0, 4.0])):0:3);

  WriteLn(IsNan(NaN), ' ', IsNan(1.0), ' ', IsInfinite(Infinity), ' ', IsInfinite(NegInfinity),
    ' ', IsInfinite(1.0));
  for I := -1 to 1 do
    Write(I, ':', Sign(I), ' ');
  WriteLn;
end.
