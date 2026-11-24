// ===== AssertRaises and AssertNear =====

function AssertRaises<E>(const P: TProc): E;
begin
  try
    P();
  except
    on X: E do
      Exit(X);
    on X: Exception do
      // A failed expectation, not an error of the test: FAIL, with the exception attached
      Exception.RaiseOuterException(EAssertionFailed.Create(
        'expected an exception of class ' + E.ClassName + ', but ' + X.ClassName + ' was raised'));
  end;
  raise EAssertionFailed.Create('expected an exception of class ' + E.ClassName + ', but none was raised');
end;

const
  // The largest finite Double (System.Math.MaxDouble, not used here to keep the uses short)
  MaxDoubleValue = 1.7976931348623157e308;

procedure AssertNear(const Actual, Expected, Tolerance: Double);
var
  Diff: Double;
begin
  if not (Tolerance >= 0) then
    raise EArgumentException.Create('the tolerance of AssertNear is negative or NaN: ' + FloatToStr(Tolerance));
  // Equal infinities are near; a NaN is near nothing (every comparison with it is False)
  if Actual = Expected then
    Exit;
  Diff := Abs(Actual - Expected);
  // An infinite difference (an infinity and another value) is near nothing, even within an
  // infinite tolerance
  if not (Diff <= Tolerance) or (Diff > MaxDoubleValue) then
    raise EAssertionFailed.Create('expected ' + FloatToStr(Actual) + ' to be within ' + FloatToStr(Tolerance) +
      ' of ' + FloatToStr(Expected) + ', but the difference is ' + FloatToStr(Diff));
end;
