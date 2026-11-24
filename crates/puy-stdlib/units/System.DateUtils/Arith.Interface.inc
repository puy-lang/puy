// Adding to a date and time, and the distance between two.

// The value moved by a number of years, weeks, days, hours, minutes, seconds or milliseconds
// (back when negative); ERangeError when the result is outside the years 1 to 9999. IncYear is
// IncMonth by 12 months a year (the day kept within the month); the others move the
// milliseconds.
function IncYear(const AValue: TDateTime; const ANumberOfYears: Integer = 1): TDateTime;
function IncWeek(const AValue: TDateTime; const ANumberOfWeeks: Integer = 1): TDateTime;
function IncDay(const AValue: TDateTime; const ANumberOfDays: Integer = 1): TDateTime;
function IncHour(const AValue: TDateTime; const ANumberOfHours: Int64 = 1): TDateTime;
function IncMinute(const AValue: TDateTime; const ANumberOfMinutes: Int64 = 1): TDateTime;
function IncSecond(const AValue: TDateTime; const ANumberOfSeconds: Int64 = 1): TDateTime;
function IncMilliSecond(const AValue: TDateTime;
  const ANumberOfMilliSeconds: Int64 = 1): TDateTime;

// The whole periods between two values, whichever is first (never negative). Years and months
// count by the calendar: the most months M with IncMonth(the first, M) not after the second.
function YearsBetween(const ANow, AThen: TDateTime): Integer;
function MonthsBetween(const ANow, AThen: TDateTime): Integer;
function WeeksBetween(const ANow, AThen: TDateTime): Integer;
function DaysBetween(const ANow, AThen: TDateTime): Integer;
function HoursBetween(const ANow, AThen: TDateTime): Int64;
function MinutesBetween(const ANow, AThen: TDateTime): Int64;
function SecondsBetween(const ANow, AThen: TDateTime): Int64;
function MilliSecondsBetween(const ANow, AThen: TDateTime): Int64;

// The distance between two values in a unit, with its fraction (never negative).
function WeekSpan(const ANow, AThen: TDateTime): Double;
function DaySpan(const ANow, AThen: TDateTime): Double;
function HourSpan(const ANow, AThen: TDateTime): Double;
function MinuteSpan(const ANow, AThen: TDateTime): Double;
function SecondSpan(const ANow, AThen: TDateTime): Double;
function MilliSecondSpan(const ANow, AThen: TDateTime): Double;
