program DateTimeEncode;

// Encoding and decoding dates and times, the day of the week, leap years, IncMonth, and
// FormatDateTime with specifiers that do not depend on the locale.

uses
  System.SysUtils;

procedure Show(const D: TDateTime);
var
  Y, M, Day, H, N, S, Ms: Word;
begin
  DecodeDate(D, Y, M, Day);
  DecodeTime(D, H, N, S, Ms);
  WriteLn(Y, ' ', M, ' ', Day, ' ', H, ' ', N, ' ', S, ' ', Ms, ' ', DayOfWeek(D));
end;

var
  D, T: TDateTime;
  Y, M, Day, Dow: Word;
begin
  D := EncodeDate(2026, 11, 1);
  WriteLn(Trunc(D));
  Show(D);
  Show(D + EncodeTime(13, 5, 9, 250));
  Show(EncodeDate(1899, 12, 30));
  Show(EncodeDate(1, 1, 1));
  Show(EncodeDate(9999, 12, 31) + EncodeTime(23, 59, 59, 999));
  Show(EncodeDate(1970, 1, 1));
  Show(EncodeDate(2000, 2, 29));
  WriteLn(Trunc(EncodeDate(1, 1, 1)), ' ', Trunc(EncodeDate(9999, 12, 31)));

  WriteLn(IsLeapYear(2000), ' ', IsLeapYear(1900), ' ', IsLeapYear(2024), ' ', IsLeapYear(2026));
  WriteLn(TryEncodeDate(2026, 2, 29, T), ' ', TryEncodeDate(2024, 2, 29, T), ' ',
    TryEncodeDate(2026, 13, 1, T), ' ', TryEncodeDate(2026, 4, 31, T), ' ',
    TryEncodeDate(0, 1, 1, T));
  WriteLn(TryEncodeTime(23, 59, 59, 999, T), ' ', TryEncodeTime(24, 0, 0, 0, T), ' ',
    TryEncodeTime(0, 60, 0, 0, T), ' ', TryEncodeTime(0, 0, 0, 1000, T));
  WriteLn(DecodeDateFully(EncodeDate(2024, 3, 1), Y, M, Day, Dow), ' ', Y, ' ', M, ' ', Day,
    ' ', Dow);

  // IncMonth keeps the day within the month and keeps the time
  Show(IncMonth(EncodeDate(2024, 1, 31)));
  Show(IncMonth(EncodeDate(2023, 1, 31) + EncodeTime(6, 0, 0, 0)));
  Show(IncMonth(EncodeDate(2026, 3, 15), -3));
  Show(IncMonth(EncodeDate(2026, 3, 15), 25));

  T := EncodeDate(2026, 11, 1) + EncodeTime(10, 20, 30, 400);
  ReplaceTime(T, EncodeTime(1, 2, 3, 4));
  Show(T);
  ReplaceDate(T, EncodeDate(2020, 5, 6));
  Show(T);

  T := EncodeDate(2026, 3, 7) + EncodeTime(14, 5, 9, 45);
  WriteLn(FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', T));
  WriteLn(FormatDateTime('yy m d h n s z', T));
  WriteLn(FormatDateTime('hh:mm', T));
  WriteLn(FormatDateTime('h:nn am/pm', T), ' ', FormatDateTime('h:nn AM/PM', EncodeTime(0, 5, 0, 0)));
  WriteLn(FormatDateTime('h a/p', T), ' ', FormatDateTime('hh A/P', EncodeTime(12, 0, 0, 0)));
  WriteLn(FormatDateTime('"yyyy" ''mm'' yyyy', T));
  // The date alone when the time is midnight
  WriteLn(Length(DateTimeToStr(D)) = Length(DateToStr(D)), ' ',
    Length(DateTimeToStr(D + 0.5)) > Length(DateToStr(D)));
end.
