program DateUtilsEdge;

// The rules of System.DateUtils that differ from FPC or that FPC does not have: values before
// 1899-12-30, months counted by the calendar, the time 24:00, the failed TryEncodeDateTime,
// ISO 8601 text in its other forms, and the errors at the ends of the years 1 to 9999.

uses
  System.SysUtils, System.DateUtils;

procedure Show(const Name: string; const D: TDateTime);
begin
  WriteLn(Name, ' ', FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', D), ' (', D:0:6, ')');
end;

procedure Iso(const S: string);
var
  D: TDateTime;
begin
  if TryISO8601ToDate(S, D) then
    WriteLn('''', S, ''' ', FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', D))
  else
    WriteLn('''', S, ''' fails');
end;

procedure Fails(const Name: string; const P: TProc);
begin
  try
    P();
    WriteLn(Name, ': no exception');
  except
    on E: Exception do
      WriteLn(Name, ': ', E.ClassName, ': ', E.Message);
  end;
end;

var
  D: TDateTime;
begin
  // Before 1899-12-30 the fraction counts away from zero: -1.25 is 1899-12-29 06:00
  D := EncodeDateTime(1899, 12, 29, 6, 0, 0, 0);
  Show('before', D);
  Show('IncHour 1', IncHour(D, 1));
  Show('IncHour 20', IncHour(D, 20));
  Show('IncMinute -1', IncMinute(D, -1));
  Show('DateOf', DateOf(D));
  Show('TimeOf', TimeOf(D));
  Show('EndOfTheDay', EndOfTheDay(D));
  WriteLn(HoursBetween(D, EncodeDateTime(1899, 12, 30, 6, 0, 0, 0)), ' ',
    DayOfTheWeek(D), ' ', DayOfTheYear(D));

  // Months and years by the calendar: the most months M with IncMonth(first, M) not after the
  // second
  WriteLn(MonthsBetween(EncodeDate(2024, 1, 31), EncodeDate(2024, 2, 29)), ' ',
    MonthsBetween(EncodeDate(2024, 1, 31), EncodeDate(2024, 2, 28)), ' ',
    MonthsBetween(EncodeDate(2024, 3, 15), EncodeDate(2023, 1, 15)), ' ',
    MonthsBetween(EncodeDateTime(2024, 1, 15, 12, 0, 0, 0), EncodeDateTime(2024, 2, 15, 11, 0, 0, 0)),
    ' ', YearsBetween(EncodeDate(2020, 2, 29), EncodeDate(2024, 2, 28)), ' ',
    YearsBetween(EncodeDate(2020, 2, 29), EncodeDate(2024, 2, 29)), ' ',
    YearsBetween(EncodeDate(2023, 2, 28), EncodeDate(2024, 2, 29)));

  // The time 24:00 is not valid (as EncodeTime); a failed TryEncodeDateTime gives 0
  WriteLn(IsValidTime(24, 0, 0, 0), ' ', IsValidDateTime(2024, 1, 1, 24, 0, 0, 0));
  D := 1;
  WriteLn(TryEncodeDateTime(2024, 2, 30, 0, 0, 0, 0, D), ' ', D:0:1);

  // ISO 8601: the forms read, and the ones not
  Iso('2026-11-20');
  Iso('2026-11-20T12:34');
  Iso('2026-11-20t12:34:56z');
  Iso('2026-11-20 12:34:56.7');
  Iso('2026-11-20T12:34:56.123456789');
  Iso('2026-11-20T12:34:56+0930');
  Iso('2026-11-20T12:34:56-02');
  Iso('0001-01-01T00:00:00Z');
  Iso('9999-12-31T23:59:59.999Z');
  Iso('0001-01-01T00:00:00+00:01');
  Iso('9999-12-31T23:59:59-00:01');
  Iso('2026-11-20T24:00:00Z');
  Iso('2026-02-30');
  Iso('26-11-20');
  Iso('2026-11-20T12');
  Iso('2026-11-20T12:34:56.');
  Iso('2026-11-20T12:34:56+24:00');
  Iso('2026-11-20Z');
  Iso(' 2026-11-20');
  Iso('2026-11-20T12:34:56Zx');
  WriteLn(DateToISO8601(EncodeDate(1, 1, 1)), ' ', DateToISO8601(EndOfAYear(9999)));

  Fails('ISO8601ToDate', procedure begin ISO8601ToDate('2026-13-01') end);
  Fails('IncDay', procedure begin IncDay(EncodeDate(9999, 12, 31)) end);
  Fails('IncDay huge', procedure begin IncDay(EncodeDate(2000, 1, 1), MaxInt) end);
  Fails('IncMilliSecond', procedure begin IncMilliSecond(EncodeDate(1, 1, 1), -1) end);
  Fails('IncHour huge', procedure begin IncHour(EncodeDate(2000, 1, 1), High(Int64)) end);
  Fails('IncYear', procedure begin IncYear(EncodeDate(2000, 1, 1), 8000) end);
  Fails('IncYear huge', procedure begin IncYear(EncodeDate(2000, 1, 1), -MaxInt) end);
  Fails('EndOfTheWeek', procedure begin EndOfTheWeek(EncodeDate(9999, 12, 31)) end);
  Fails('YearOf', procedure begin YearOf(3000000) end);
  Fails('UnixToDateTime', procedure begin UnixToDateTime(253402300800) end);
  Fails('UnixToDateTime huge', procedure begin UnixToDateTime(Low(Int64)) end);
  Fails('EndOfAMonth', procedure begin EndOfAMonth(2024, 13) end);
  Show('UnixToDateTime', UnixToDateTime(253402300799));
  Show('UnixToDateTime', UnixToDateTime(-62135596800));
end.
