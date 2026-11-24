program DateUtilsParts;

// The parts of a date and time, the ISO 8601 day of the week and week of the year, the days of
// months and years, the start and the end of a period, validity and comparisons.

uses
  System.SysUtils, System.DateUtils;

procedure Show(const Name: string; const D: TDateTime);
begin
  WriteLn(Name, ' ', FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', D));
end;

procedure Week(const D: TDateTime);
var
  Year, W: Word;
begin
  W := WeekOfTheYear(D, Year);
  WriteLn(FormatDateTime('yyyy-mm-dd', D), ' dow ', DayOfTheWeek(D), ' week ', W, ' of ', Year,
    ' (', WeekOf(D), ')');
end;

var
  D, A, B, C: TDateTime;
  Y, M, Day, H, N, S, Ms: Word;
begin
  D := EncodeDateTime(2024, 2, 29, 13, 45, 30, 250);
  WriteLn(YearOf(D), ' ', MonthOf(D), ' ', DayOf(D), ' ', HourOf(D), ' ', MinuteOf(D), ' ',
    SecondOf(D), ' ', MilliSecondOf(D));
  DecodeDateTime(D, Y, M, Day, H, N, S, Ms);
  WriteLn(Y, '-', M, '-', Day, ' ', H, ':', N, ':', S, '.', Ms);
  WriteLn(DayOfTheYear(D), ' ', DayOfTheYear(EncodeDate(2023, 12, 31)), ' ',
    DayOfTheYear(EncodeDate(2024, 12, 31)));
  WriteLn(DaysInMonth(D), ' ', DaysInYear(D), ' ', DaysInAMonth(2023, 2), ' ', DaysInAMonth(2000, 2),
    ' ', DaysInAMonth(1900, 2), ' ', DaysInAMonth(2024, 4), ' ', DaysInAYear(2023), ' ',
    DaysInAYear(2000));
  WriteLn(IsInLeapYear(D), ' ', IsInLeapYear(EncodeDate(2100, 6, 1)), ' ', IsPM(D), ' ',
    IsPM(EncodeTime(11, 59, 59, 999)), ' ', IsPM(EncodeTime(12, 0, 0, 0)));

  // ISO 8601 weeks: Monday is 1; the week of a Thursday decides the year
  Week(EncodeDate(2024, 1, 1));
  Week(EncodeDate(2024, 12, 29));
  Week(EncodeDate(2024, 12, 30));
  Week(EncodeDate(2021, 1, 3));
  Week(EncodeDate(2021, 1, 4));
  Week(EncodeDate(2020, 12, 31));
  Week(EncodeDate(2026, 11, 22));
  WriteLn(DayMonday, ' ', DaySunday, ' ', WeeksInAYear(2015), ' ', WeeksInAYear(2020), ' ',
    WeeksInAYear(2021), ' ', WeeksInAYear(2026), ' ', WeeksInYear(D));

  Show('StartOfTheDay', StartOfTheDay(D));
  Show('EndOfTheDay', EndOfTheDay(D));
  Show('StartOfTheWeek', StartOfTheWeek(D));
  Show('EndOfTheWeek', EndOfTheWeek(D));
  Show('StartOfTheMonth', StartOfTheMonth(D));
  Show('EndOfTheMonth', EndOfTheMonth(D));
  Show('StartOfTheYear', StartOfTheYear(D));
  Show('EndOfTheYear', EndOfTheYear(D));
  Show('StartOfAMonth', StartOfAMonth(2023, 2));
  Show('EndOfAMonth', EndOfAMonth(2023, 2));
  Show('StartOfAYear', StartOfAYear(1999));
  Show('EndOfAYear', EndOfAYear(1999));
  Show('DateOf', DateOf(D));
  Show('TimeOf', TimeOf(D));

  WriteLn(IsValidDate(2024, 2, 29), ' ', IsValidDate(2023, 2, 29), ' ', IsValidDate(2024, 13, 1),
    ' ', IsValidDate(0, 1, 1), ' ', IsValidTime(23, 59, 59, 999), ' ', IsValidTime(23, 60, 0, 0),
    ' ', IsValidDateTime(2024, 2, 29, 12, 0, 0, 0), ' ', IsValidDateTime(2024, 2, 29, 12, 60, 0, 0));
  WriteLn(TryEncodeDateTime(2024, 2, 30, 0, 0, 0, 0, D), ' ', TryEncodeDateTime(2024, 2, 29, 23, 59, 59, 999, D));

  A := EncodeDateTime(2024, 5, 1, 10, 0, 0, 0);
  B := EncodeDateTime(2024, 5, 1, 9, 0, 0, 0);
  C := EncodeDateTime(2024, 4, 30, 10, 0, 0, 0);
  WriteLn(CompareDateTime(A, B), ' ', CompareDateTime(B, A), ' ', CompareDateTime(A, A), ' ',
    CompareDate(A, B), ' ', CompareDate(C, A), ' ', CompareTime(A, C), ' ', CompareTime(B, C));
  WriteLn(SameDateTime(A, B), ' ', SameDate(A, B), ' ', SameTime(A, C), ' ', SameTime(A, B), ' ',
    IsSameDay(A, B), ' ', IsSameDay(A, C));
end.
