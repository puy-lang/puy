program DateUtilsArith;

// Adding years to milliseconds to a date and time, and the whole periods and the spans between
// two (in either order).

uses
  System.SysUtils, System.DateUtils;

procedure Show(const Name: string; const D: TDateTime);
begin
  WriteLn(Name, ' ', FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', D));
end;

procedure Between(const A, B: TDateTime);
begin
  WriteLn(YearsBetween(A, B), ' ', MonthsBetween(A, B), ' ', WeeksBetween(A, B), ' ',
    DaysBetween(A, B), ' ', HoursBetween(A, B), ' ', MinutesBetween(A, B), ' ',
    SecondsBetween(A, B), ' ', MilliSecondsBetween(A, B));
end;

var
  D, A, B: TDateTime;
begin
  D := EncodeDateTime(2024, 2, 29, 13, 45, 30, 250);
  Show('IncYear', IncYear(D));
  Show('IncYear 4', IncYear(D, 4));
  Show('IncYear -1', IncYear(D, -1));
  Show('IncMonth', IncMonth(D));
  Show('IncWeek', IncWeek(D));
  Show('IncWeek -3', IncWeek(D, -3));
  Show('IncDay', IncDay(D));
  Show('IncDay -60', IncDay(D, -60));
  Show('IncHour', IncHour(D));
  Show('IncHour 11', IncHour(D, 11));
  Show('IncHour -14', IncHour(D, -14));
  Show('IncMinute 15', IncMinute(D, 15));
  Show('IncMinute -1000', IncMinute(D, -1000));
  Show('IncSecond 30', IncSecond(D, 30));
  Show('IncSecond 86400', IncSecond(D, 86400));
  Show('IncMilliSecond 750', IncMilliSecond(D, 750));
  Show('IncMilliSecond -251', IncMilliSecond(D, -251));

  Between(EncodeDate(2020, 1, 1), EncodeDate(2024, 1, 1));
  Between(EncodeDate(2024, 3, 20), EncodeDate(2023, 1, 1));
  Between(EncodeDateTime(2024, 1, 1, 0, 0, 0, 0), EncodeDateTime(2024, 1, 2, 12, 30, 15, 500));
  Between(EncodeDateTime(2024, 6, 1, 23, 0, 0, 0), EncodeDateTime(2024, 6, 2, 1, 0, 0, 0));
  Between(D, D);

  A := EncodeDateTime(2024, 1, 1, 0, 0, 0, 0);
  B := EncodeDateTime(2024, 1, 11, 6, 0, 0, 0);
  WriteLn(WeekSpan(A, B):0:6, ' ', DaySpan(A, B):0:6, ' ', HourSpan(B, A):0:6, ' ',
    MinuteSpan(A, B):0:3, ' ', SecondSpan(A, B):0:3, ' ', MilliSecondSpan(A, B):0:1);
end.
