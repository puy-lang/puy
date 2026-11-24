// The start and the end of a day, a week, a month and a year.

// The last millisecond of the day Days.
function DuEndOfDay(const Days: Int64): TDateTime;
begin
  Result := DuFromMillis((Days + 1) * MSecsPerDay - 1);
end;

function StartOfTheDay(const AValue: TDateTime): TDateTime;
begin
  Result := DuDays(DuToMillis(AValue));
end;

function EndOfTheDay(const AValue: TDateTime): TDateTime;
begin
  Result := DuEndOfDay(DuDays(DuToMillis(AValue)));
end;

function StartOfTheWeek(const AValue: TDateTime): TDateTime;
begin
  var Days := DuDays(DuToMillis(AValue));
  Result := DuFromMillis((Days - DuIsoDayOfWeek(Days) + DayMonday) * MSecsPerDay);
end;

function EndOfTheWeek(const AValue: TDateTime): TDateTime;
begin
  var Days := DuDays(DuToMillis(AValue));
  Result := DuEndOfDay(Days - DuIsoDayOfWeek(Days) + DaySunday);
end;

function StartOfAMonth(const AYear, AMonth: Word): TDateTime;
begin
  Result := EncodeDate(AYear, AMonth, 1);
end;

function EndOfAMonth(const AYear, AMonth: Word): TDateTime;
begin
  if (AMonth < 1) or (AMonth > MonthsPerYear) then
    // EncodeDate says the month is not valid
    Result := EncodeDate(AYear, AMonth, 1)
  else
    Result := DuEndOfDay(Trunc(EncodeDate(AYear, AMonth, DaysInAMonth(AYear, AMonth))));
end;

function StartOfAYear(const AYear: Word): TDateTime;
begin
  Result := EncodeDate(AYear, 1, 1);
end;

function EndOfAYear(const AYear: Word): TDateTime;
begin
  Result := DuEndOfDay(Trunc(EncodeDate(AYear, 12, 31)));
end;

function StartOfTheMonth(const AValue: TDateTime): TDateTime;
begin
  var P := DuParts(AValue);
  Result := StartOfAMonth(P.Year, P.Month);
end;

function EndOfTheMonth(const AValue: TDateTime): TDateTime;
begin
  var P := DuParts(AValue);
  Result := EndOfAMonth(P.Year, P.Month);
end;

function StartOfTheYear(const AValue: TDateTime): TDateTime;
begin
  Result := StartOfAYear(YearOf(AValue));
end;

function EndOfTheYear(const AValue: TDateTime): TDateTime;
begin
  Result := EndOfAYear(YearOf(AValue));
end;
