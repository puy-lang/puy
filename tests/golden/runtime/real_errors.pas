program RealErrors;

// The math exceptions (types.md 8.4, exceptions.md chapter 2): `EInvalidOp` from `Trunc` /
// `Round` of NaN, an infinity or a value outside Int64, `EOverflow` from Currency arithmetic
// and conversions (whatever `{$Q}` says), `EZeroDivide` from a Currency divisor of 0, all
// caught by `on EMathError`; real arithmetic itself never raises. The last one is uncaught.

{$Q-}

var
  D, Z: Double;
  C, Y: Currency;
  N: Int64;

procedure Attempt(Step: Integer);
begin
  try
    case Step of
      1: N := Trunc(Z / Z);
      2: N := Round(D);
      3: N := Trunc(-D);
      4: N := Round(9.3E18);
      5: C := C * 2;
      6: C := C + C;
      7: C := -C - C;
      8: C := D;
      9: C := Z / Z;
      10: C := N;
      11: C := C / Y;
      12:
        begin
          Y := -C - Currency(0.5808);
          WriteLn(Y);
          Y := Abs(Y);
        end;
    end;
    WriteLn(Step, ': no exception');
  except
    on E: EMathError do
      WriteLn(Step, ': ', E.ClassName, ': ', E.Message);
  end;
end;

var
  I: Integer;
begin
  Z := 0;
  D := 1 / Z;
  C := 922337203685477;
  Y := 0;
  N := 9223372036854775807;
  WriteLn(D * 0, ' ', D - D, ' ', 1E308 * 10);
  for I := 1 to 12 do
    Attempt(I);
  WriteLn(Round(-9.2E18), ' ', Trunc(9.2E18));
  WriteLn(Round(Currency(-2.5)), ' ', Trunc(Currency(-2.5)));
  C := 1;
  Y := 0;
  WriteLn(C / Y);
end.
