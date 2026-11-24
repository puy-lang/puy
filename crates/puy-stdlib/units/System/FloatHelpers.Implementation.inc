// ===== Floating-point type helpers (docs/spec/stdlib/System/Helpers.md) =====

// Whether S reads as StrToFloat reads it (StrToFloat without raising).
function TryParseFloat(const S: string; out Value: Double): Boolean;
begin
  Value := 0;
  try
    Value := StrToFloat(S);
  except
    on EConvertError do
      Exit(False);
  end;
  Result := True;
end;

// --- TSingleHelper ---

class function TSingleHelper.FromBits(Bits: Cardinal): Single;
begin
  Result := BitsToSingle(Bits);
end;

class function TSingleHelper.Parse(const S: string): Single;
begin
  Result := StrToFloat(S);
end;

class function TSingleHelper.TryParse(const S: string; out Value: Single): Boolean;
var
  V: Double;
begin
  Result := TryParseFloat(S, V);
  Value := V;
end;

function TSingleHelper.IsInfinity: Boolean;
begin
  Result := (Self = PositiveInfinity) or (Self = NegativeInfinity);
end;

function TSingleHelper.IsNan: Boolean;
begin
  // NaN is the only value that differs from itself
  Result := Self <> Self;
end;

function TSingleHelper.IsNegativeInfinity: Boolean;
begin
  Result := Self = NegativeInfinity;
end;

function TSingleHelper.IsPositiveInfinity: Boolean;
begin
  Result := Self = PositiveInfinity;
end;

function TSingleHelper.ToBits: Cardinal;
begin
  Result := SingleToBits(Self);
end;

// --- TDoubleHelper ---

class function TDoubleHelper.FromBits(Bits: UInt64): Double;
begin
  Result := BitsToDouble(Bits);
end;

class function TDoubleHelper.Parse(const S: string): Double;
begin
  Result := StrToFloat(S);
end;

class function TDoubleHelper.TryParse(const S: string; out Value: Double): Boolean;
var
  V: Double;
begin
  Result := TryParseFloat(S, V);
  Value := V;
end;

function TDoubleHelper.IsInfinity: Boolean;
begin
  Result := (Self = PositiveInfinity) or (Self = NegativeInfinity);
end;

function TDoubleHelper.IsNan: Boolean;
begin
  // NaN is the only value that differs from itself
  Result := Self <> Self;
end;

function TDoubleHelper.IsNegativeInfinity: Boolean;
begin
  Result := Self = NegativeInfinity;
end;

function TDoubleHelper.IsPositiveInfinity: Boolean;
begin
  Result := Self = PositiveInfinity;
end;

function TDoubleHelper.ToBits: UInt64;
begin
  Result := DoubleToBits(Self);
end;
