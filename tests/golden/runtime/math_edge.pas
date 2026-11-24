// System.Math beyond what the diff cases check against FPC: NaN and the infinities, the
// exceptions, the default Epsilon of SameValue, the Single overloads, the routines with `var`
// results, and the reciprocal functions (docs/spec/stdlib/System.Math/README.md).
program MathEdge;

uses
  System.SysUtils, System.Math;

procedure Check(const Name: string; const P: TProc);
begin
  try
    P();
    WriteLn(Name, ': no exception');
  except
    on E: Exception do
      WriteLn(Name, ': ', E.ClassName, ': ', E.Message);
  end;
end;

var
  M, D, S, Q: Double;
  F: Single;
begin
  // NaN wins in Min / Max whichever side it is on
  WriteLn(Min(NaN, 1.0), ' ', Min(1.0, NaN), ' ', Max(NaN, 1.0), ' ', Max(1.0, NaN));
  F := NaN;
  WriteLn(Min(F, Single(1)), ' ', IsNan(F), ' ', IsNan(Single(1)));
  WriteLn(Min(Infinity, 1.0), ' ', Max(NegInfinity, 1.0));
  WriteLn(Min(UInt64(18446744073709551615), UInt64(1)), ' ', Max(UInt64(18446744073709551615), UInt64(1)));
  WriteLn(Sign(NaN), ' ', Sign(-0.0), ' ', Sign(NegInfinity));
  WriteLn(InRange(NaN, 0.0, 1.0), ' ', InRange(UInt64(5), UInt64(1), UInt64(10)));
  WriteLn(EnsureRange(NaN, 0.0, 1.0), ' ', EnsureRange(UInt64(50), UInt64(1), UInt64(10)));

  // The functions of System and of the unit at the edges of their domains
  WriteLn(Sqrt(-1), ' ', Ln(0), ' ', Ln(-1), ' ', Exp(1000), ' ', Exp(-1000));
  WriteLn(Int(Infinity), ' ', Frac(Infinity), ' ', Int(NaN), ' ', Frac(-0.0));
  WriteLn(ArcSin(2), ' ', ArcCosh(0.5), ' ', ArcTanh(1), ' ', Log10(0), ' ', Log2(-1));
  WriteLn(Power(0, 0), ' ', Power(NaN, 0), ' ', Power(-8, 1 / 3), ' ', Power(-2, 3), ' ', Power(0, -1));
  WriteLn(IntPower(2, 1024), ' ', IntPower(2, -1074), ' ', IntPower(-1, Low(Integer)));
  WriteLn(ArcTan2(0, -1), ' ', ArcTan2(-0.0, -1), ' ', Hypot(Infinity, NaN));

  // Cot, Sec, Csc and the inverse functions of the reciprocals
  WriteLn(Cot(1):0:9, ' ', Sec(1):0:9, ' ', Csc(1):0:9);
  WriteLn(ArcCot(2):0:9, ' ', ArcSec(2):0:9, ' ', ArcCsc(2):0:9);
  WriteLn(CotH(1):0:9, ' ', SecH(1):0:9, ' ', CscH(1):0:9);
  WriteLn(ArcCotH(2):0:9, ' ', ArcSecH(0.5):0:9, ' ', ArcCscH(2):0:9);

  // Rounding: a value too large to scale is itself, and the infinities stay
  WriteLn(RoundTo(1E300, -20), ' ', RoundTo(Infinity, -2), ' ', SimpleRoundTo(-2.5, 0):0:1,
    ' ', SimpleRoundTo(-0.125, -2):0:4, ' ', RoundTo(-0.125, -2):0:4);

  // The default Epsilon is relative to the smaller magnitude
  WriteLn(SameValue(1000000.0, 1000000.0000001), ' ', SameValue(1E-13, 0.0),
    ' ', SameValue(Infinity, Infinity), ' ', SameValue(NaN, NaN));
  WriteLn(SameValue(Single(1), Single(1.00001)), ' ', SameValue(Single(1), Single(1.001)),
    ' ', IsZero(Single(0.00001)), ' ', IsZero(Single(0.001)));
  WriteLn(CompareValue(1.0, 1.0 + 1E-13), ' ', CompareValue(Infinity, 1.0),
    ' ', CompareValue(UInt64(1), UInt64(18446744073709551615)));
  WriteLn(IfThen(False, 5), ' ', IfThen(True, Int64(10000000000)), ' ', IfThen(False, 1.5):0:1);

  MeanAndStdDev([1.0, 2.0, 3.0, 4.0], M, D);
  WriteLn(M:0:3, ' ', D:0:6);
  SumsAndSquares([1.0, 2.0, 3.0], S, Q);
  WriteLn(S:0:2, ' ', Q:0:2);
  WriteLn(Sum([]), ' ', SumInt([]), ' ', Variance([5.0]), ' ', StdDev([5.0]), ' ', Norm([]));
  WriteLn(MinValue([1.0, NaN, 0.5]), ' ', MaxValue([Infinity, 1.0]));

  WriteLn(MinSingle, ' ', MaxSingle, ' ', MinDouble, ' ', MaxDouble, ' ', MaxExtended);

  Check('Floor(NaN)', procedure begin WriteLn(Floor(NaN)); end);
  Check('Floor(1E10)', procedure begin WriteLn(Floor(1E10)); end);
  Check('Ceil(-1E10)', procedure begin WriteLn(Ceil(-1E10)); end);
  Check('Floor(-2147483648.5)', procedure begin WriteLn(Floor(-2147483648.5)); end);
  Check('Ceil(2147483647.5)', procedure begin WriteLn(Ceil(2147483647.5)); end);
  Check('Ceil(-2147483648.5)', procedure begin WriteLn(Ceil(-2147483648.5)); end);
  Check('EnsureRange', procedure begin WriteLn(EnsureRange(5, 10, 1)); end);
  Check('EnsureRange(NaN)', procedure begin WriteLn(EnsureRange(0.5, NaN, 1.0)); end);
  Check('Mean', procedure begin WriteLn(Mean([])); end);
  Check('MinValue', procedure begin WriteLn(MinValue([])); end);
  Check('MaxIntValue', procedure begin WriteLn(MaxIntValue([])); end);
  Check('PopnVariance', procedure begin WriteLn(PopnVariance([])); end);
  Check('CompareValue(NaN)', procedure begin WriteLn(CompareValue(NaN, 1.0)); end);
  Check('SumInt', procedure begin WriteLn(SumInt([MaxInt, 1])); end);
  Check('EInvalidArgument is an EMathError', procedure
    begin
      try
        Mean([]);
      except
        on E: EMathError do
          WriteLn(E is EInvalidArgument);
      end;
    end);
end.
