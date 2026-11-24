// ===== Sums and statistics of arrays =====

// EInvalidArgument for a routine given an empty array.
procedure CheckNotEmpty(Count: Integer; const Name: string);
begin
  if Count = 0 then
    raise EInvalidArgument.Create(Name + ': the array is empty');
end;

function Sum(const Data: array of Double): Double;
var
  I: Integer;
begin
  Result := 0;
  for I := 0 to High(Data) do
    Result := Result + Data[I];
end;

// Summed in an Int64, which cannot overflow; a sum outside Integer raises EIntOverflow
// whatever {$Q} is.
function SumInt(const Data: array of Integer): Integer;
var
  I: Integer;
  S: Int64;
begin
  S := 0;
  for I := 0 to High(Data) do
    S := S + Data[I];
  if (S < Low(Integer)) or (S > High(Integer)) then
    raise EIntOverflow.Create('integer overflow in SumInt');
  Result := S;
end;

function SumOfSquares(const Data: array of Double): Double;
var
  I: Integer;
begin
  Result := 0;
  for I := 0 to High(Data) do
    Result := Result + Data[I] * Data[I];
end;

procedure SumsAndSquares(const Data: array of Double; var Sum, SumOfSquares: Double);
var
  I: Integer;
begin
  Sum := 0;
  SumOfSquares := 0;
  for I := 0 to High(Data) do
  begin
    Sum := Sum + Data[I];
    SumOfSquares := SumOfSquares + Data[I] * Data[I];
  end;
end;

function Mean(const Data: array of Double): Double;
begin
  CheckNotEmpty(Length(Data), 'Mean');
  Result := Sum(Data) / Length(Data);
end;

function MinValue(const Data: array of Double): Double;
var
  I: Integer;
begin
  CheckNotEmpty(Length(Data), 'MinValue');
  Result := Data[0];
  for I := 1 to High(Data) do
    Result := Min(Result, Data[I]);
end;

function MaxValue(const Data: array of Double): Double;
var
  I: Integer;
begin
  CheckNotEmpty(Length(Data), 'MaxValue');
  Result := Data[0];
  for I := 1 to High(Data) do
    Result := Max(Result, Data[I]);
end;

function MinIntValue(const Data: array of Integer): Integer;
var
  I: Integer;
begin
  CheckNotEmpty(Length(Data), 'MinIntValue');
  Result := Data[0];
  for I := 1 to High(Data) do
    if Data[I] < Result then
      Result := Data[I];
end;

function MaxIntValue(const Data: array of Integer): Integer;
var
  I: Integer;
begin
  CheckNotEmpty(Length(Data), 'MaxIntValue');
  Result := Data[0];
  for I := 1 to High(Data) do
    if Data[I] > Result then
      Result := Data[I];
end;

// The sum of the squared differences from the mean (two passes, so that a large mean does not
// cancel the digits of the differences).
function SquaredDeviations(const Data: array of Double): Double;
var
  M, D: Double;
  I: Integer;
begin
  M := Sum(Data) / Length(Data);
  Result := 0;
  for I := 0 to High(Data) do
  begin
    D := Data[I] - M;
    Result := Result + D * D;
  end;
end;

function Variance(const Data: array of Double): Double;
begin
  CheckNotEmpty(Length(Data), 'Variance');
  if Length(Data) = 1 then
    Result := 0
  else
    Result := SquaredDeviations(Data) / (Length(Data) - 1);
end;

function StdDev(const Data: array of Double): Double;
begin
  CheckNotEmpty(Length(Data), 'StdDev');
  Result := Sqrt(Variance(Data));
end;

function PopnVariance(const Data: array of Double): Double;
begin
  CheckNotEmpty(Length(Data), 'PopnVariance');
  Result := SquaredDeviations(Data) / Length(Data);
end;

function PopnStdDev(const Data: array of Double): Double;
begin
  CheckNotEmpty(Length(Data), 'PopnStdDev');
  Result := Sqrt(PopnVariance(Data));
end;

procedure MeanAndStdDev(const Data: array of Double; var Mean, StdDev: Double);
begin
  CheckNotEmpty(Length(Data), 'MeanAndStdDev');
  Mean := Sum(Data) / Length(Data);
  StdDev := System.Math.StdDev(Data);
end;

function Norm(const Data: array of Double): Double;
begin
  Result := Sqrt(SumOfSquares(Data));
end;
