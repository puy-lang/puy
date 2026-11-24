// Adding to a date and time, and the distance between two.

function IncYear(const AValue: TDateTime; const ANumberOfYears: Integer): TDateTime;
begin
  // More than 9999 years is out of the range anyway; IncMonth then says so
  var Years := ANumberOfYears;
  if Years > 10000 then
    Years := 10000
  else if Years < -10000 then
    Years := -10000;
  Result := IncMonth(AValue, Years * MonthsPerYear);
end;

function IncWeek(const AValue: TDateTime; const ANumberOfWeeks: Integer): TDateTime;
begin
  Result := DuAddMillis(DuToMillis(AValue), ANumberOfWeeks, DuMillisPerWeek);
end;

function IncDay(const AValue: TDateTime; const ANumberOfDays: Integer): TDateTime;
begin
  Result := DuAddMillis(DuToMillis(AValue), ANumberOfDays, MSecsPerDay);
end;

function IncHour(const AValue: TDateTime; const ANumberOfHours: Int64): TDateTime;
begin
  Result := DuAddMillis(DuToMillis(AValue), ANumberOfHours, DuMillisPerHour);
end;

function IncMinute(const AValue: TDateTime; const ANumberOfMinutes: Int64): TDateTime;
begin
  Result := DuAddMillis(DuToMillis(AValue), ANumberOfMinutes, DuMillisPerMinute);
end;

function IncSecond(const AValue: TDateTime; const ANumberOfSeconds: Int64): TDateTime;
begin
  Result := DuAddMillis(DuToMillis(AValue), ANumberOfSeconds, MSecsPerSec);
end;

function IncMilliSecond(const AValue: TDateTime; const ANumberOfMilliSeconds: Int64): TDateTime;
begin
  Result := DuAddMillis(DuToMillis(AValue), ANumberOfMilliSeconds, 1);
end;

// The milliseconds between two values (never negative).
function DuMillisBetween(const ANow, AThen: TDateTime): Int64;
begin
  Result := Abs(DuToMillis(ANow) - DuToMillis(AThen));
end;

function MonthsBetween(const ANow, AThen: TDateTime): Integer;
begin
  var First := ANow;
  var Last := AThen;
  if DuToMillis(First) > DuToMillis(Last) then
  begin
    First := AThen;
    Last := ANow;
  end;
  var F := DuParts(First);
  var L := DuParts(Last);
  Result := (L.Year * MonthsPerYear + L.Month) - (F.Year * MonthsPerYear + F.Month);
  // The months of the calendar are whole unless the day and time of the last are before the
  // ones the first moves to
  if (Result > 0) and (DuToMillis(IncMonth(First, Result)) > DuToMillis(Last)) then
    Dec(Result);
end;

function YearsBetween(const ANow, AThen: TDateTime): Integer;
begin
  Result := MonthsBetween(ANow, AThen) div MonthsPerYear;
end;

function WeeksBetween(const ANow, AThen: TDateTime): Integer;
begin
  Result := DuMillisBetween(ANow, AThen) div DuMillisPerWeek;
end;

function DaysBetween(const ANow, AThen: TDateTime): Integer;
begin
  Result := DuMillisBetween(ANow, AThen) div MSecsPerDay;
end;

function HoursBetween(const ANow, AThen: TDateTime): Int64;
begin
  Result := DuMillisBetween(ANow, AThen) div DuMillisPerHour;
end;

function MinutesBetween(const ANow, AThen: TDateTime): Int64;
begin
  Result := DuMillisBetween(ANow, AThen) div DuMillisPerMinute;
end;

function SecondsBetween(const ANow, AThen: TDateTime): Int64;
begin
  Result := DuMillisBetween(ANow, AThen) div MSecsPerSec;
end;

function MilliSecondsBetween(const ANow, AThen: TDateTime): Int64;
begin
  Result := DuMillisBetween(ANow, AThen);
end;

function WeekSpan(const ANow, AThen: TDateTime): Double;
begin
  Result := DuMillisBetween(ANow, AThen) / DuMillisPerWeek;
end;

function DaySpan(const ANow, AThen: TDateTime): Double;
begin
  Result := DuMillisBetween(ANow, AThen) / MSecsPerDay;
end;

function HourSpan(const ANow, AThen: TDateTime): Double;
begin
  Result := DuMillisBetween(ANow, AThen) / DuMillisPerHour;
end;

function MinuteSpan(const ANow, AThen: TDateTime): Double;
begin
  Result := DuMillisBetween(ANow, AThen) / DuMillisPerMinute;
end;

function SecondSpan(const ANow, AThen: TDateTime): Double;
begin
  Result := DuMillisBetween(ANow, AThen) / MSecsPerSec;
end;

function MilliSecondSpan(const ANow, AThen: TDateTime): Double;
begin
  Result := DuMillisBetween(ANow, AThen);
end;
