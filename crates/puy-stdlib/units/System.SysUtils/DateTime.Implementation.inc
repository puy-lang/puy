// Dates and times as TDateTime (docs/spec/stdlib/System.SysUtils/DateTime.md). Every routine
// goes through the milliseconds since 1899-12-30 (DtToMillis / DtFromMillis), so that a value is
// rounded to the millisecond once and in one way. The calendar is the proleptic Gregorian one.
// (System's TInstant has its own copy of the calendar: System does not see this unit.)

const
  // The fixed formats (requirements §5.11)
  DtShortDateFormat = 'yyyy-mm-dd';
  DtLongDateFormat = 'dddd, mmmm d, yyyy';
  DtShortTimeFormat = 'hh:nn';
  DtLongTimeFormat = 'hh:nn:ss';
  DtDateSeparator = '-';
  DtTimeSeparator = ':';

// The English name of the month (1 to 12): its first three letters when not Long.
function DtMonthName(Month: Integer; Long: Boolean): string;
begin
  case Month of
    1: Result := 'January';
    2: Result := 'February';
    3: Result := 'March';
    4: Result := 'April';
    5: Result := 'May';
    6: Result := 'June';
    7: Result := 'July';
    8: Result := 'August';
    9: Result := 'September';
    10: Result := 'October';
    11: Result := 'November';
  else
    Result := 'December';
  end;
  if not Long then
    Result := Copy(Result, 1, 3);
end;

// The English name of the day of the week (1, Sunday, to 7): its first three letters when not
// Long.
function DtDayName(DayOfWeek: Integer; Long: Boolean): string;
begin
  case DayOfWeek of
    1: Result := 'Sunday';
    2: Result := 'Monday';
    3: Result := 'Tuesday';
    4: Result := 'Wednesday';
    5: Result := 'Thursday';
    6: Result := 'Friday';
  else
    Result := 'Saturday';
  end;
  if not Long then
    Result := Copy(Result, 1, 3);
end;

// A div B rounded toward minus infinity (B > 0).
function DtFloorDiv(const A, B: Int64): Int64;
begin
  Result := A div B;
  if (A mod B) < 0 then
    Dec(Result);
end;

// N in decimal, with zeros before it up to Width digits (N >= 0).
function DtPad(const N: Int64; Width: Integer): string;
begin
  Result := IntToStr(N);
  while Length(Result) < Width do
    Result := '0' + Result;
end;

// The milliseconds from 1899-12-30 of a TDateTime (before that day the fraction counts away
// from zero: -1.25 is 1899-12-29 06:00), rounded to the millisecond. ERangeError outside the
// years 1 to 9999 (and for NaN).
function DtToMillis(const DateTime: TDateTime): Int64;
begin
  // 0001-01-01 is -693593; a value whose Trunc is -693594 is in the year 0
  if not ((DateTime > -DateDelta) and (DateTime < 2958466)) then
    raise ERangeError.Create('TDateTime value ' + FloatToStr(DateTime) +
      ' is out of range (the years 1 to 9999)');
  var Days: Int64 := Trunc(DateTime);
  Result := Days * MSecsPerDay + Round(Abs(DateTime - Days) * MSecsPerDay);
end;

// The TDateTime of milliseconds from 1899-12-30.
function DtFromMillis(const Millis: Int64): TDateTime;
begin
  var Days := DtFloorDiv(Millis, MSecsPerDay);
  var Fraction: Double := (Millis - Days * MSecsPerDay) / MSecsPerDay;
  if Days >= 0 then
    Result := Days + Fraction
  else
    Result := Days - Fraction;
end;

function DtDaysInMonth(Year, Month: Integer): Integer;
begin
  case Month of
    4, 6, 9, 11:
      Result := 30;
    2:
      if IsLeapYear(Year) then
        Result := 29
      else
        Result := 28;
  else
    Result := 31;
  end;
end;

// The days from 1899-12-30 of a valid date Y-M-D.
function DtDaysFromCivil(Y, M, D: Int64): Int64;
begin
  if M <= 2 then
    Dec(Y);
  var Era := DtFloorDiv(Y, 400);
  var YearOfEra := Y - Era * 400;
  var MonthFromMarch := M - 3;
  if M <= 2 then
    MonthFromMarch := M + 9;
  var DayOfYear := (153 * MonthFromMarch + 2) div 5 + D - 1;
  var DayOfEra := YearOfEra * 365 + YearOfEra div 4 - YearOfEra div 100 + DayOfYear;
  Result := Era * 146097 + DayOfEra - 719468 + UnixDateDelta;
end;

// The date of Days from 1899-12-30.
procedure DtCivilFromDays(const Days: Int64; out Y, M, D: Word);
begin
  var Z := Days - UnixDateDelta + 719468;
  var Era := DtFloorDiv(Z, 146097);
  var DayOfEra := Z - Era * 146097;
  var YearOfEra := (DayOfEra - DayOfEra div 1460 + DayOfEra div 36524 - DayOfEra div 146096) div 365;
  var DayOfYear := DayOfEra - (365 * YearOfEra + YearOfEra div 4 - YearOfEra div 100);
  var MonthFromMarch := (5 * DayOfYear + 2) div 153;
  D := DayOfYear - (153 * MonthFromMarch + 2) div 5 + 1;
  if MonthFromMarch < 10 then
    M := MonthFromMarch + 3
  else
    M := MonthFromMarch - 9;
  var Year := YearOfEra + Era * 400;
  if M <= 2 then
    Inc(Year);
  Y := Year;
end;

// The local time of the host's clock in milliseconds from 1899-12-30.
function DtLocalMillis: Int64;
begin
  var Nanos := ClockNow;
  Result := DtFloorDiv(Nanos, 1000000) + Int64(LocalOffset(Nanos)) * MSecsPerSec +
    Int64(UnixDateDelta) * MSecsPerDay;
end;

function Now: TDateTime;
begin
  Result := DtFromMillis(DtLocalMillis);
end;

function Date: TDateTime;
begin
  Result := DtFloorDiv(DtLocalMillis, MSecsPerDay);
end;

function Time: TDateTime;
begin
  var Millis := DtLocalMillis;
  Result := (Millis - DtFloorDiv(Millis, MSecsPerDay) * MSecsPerDay) / MSecsPerDay;
end;

function IsLeapYear(Year: Word): Boolean;
begin
  Result := ((Year mod 4 = 0) and (Year mod 100 <> 0)) or (Year mod 400 = 0);
end;

function DayOfWeek(const DateTime: TDateTime): Word;
begin
  // 1899-12-30 was a Saturday
  var Days := DtFloorDiv(DtToMillis(DateTime), MSecsPerDay);
  Result := (Days mod 7 + 13) mod 7 + 1;
end;

function TryEncodeDate(Year, Month, Day: Word; out Date: TDateTime): Boolean;
begin
  Date := 0;
  Result := (Year >= 1) and (Year <= 9999) and (Month >= 1) and (Month <= 12) and (Day >= 1) and
    (Day <= DtDaysInMonth(Year, Month));
  if Result then
    Date := DtDaysFromCivil(Year, Month, Day);
end;

function TryEncodeTime(Hour, Min, Sec, MSec: Word; out Time: TDateTime): Boolean;
begin
  Time := 0;
  Result := (Hour < HoursPerDay) and (Min < MinsPerHour) and (Sec < SecsPerMin) and
    (MSec < MSecsPerSec);
  if Result then
    Time := (((Hour * 60 + Min) * 60 + Sec) * 1000 + MSec) / MSecsPerDay;
end;

function EncodeDate(Year, Month, Day: Word): TDateTime;
begin
  if not TryEncodeDate(Year, Month, Day, Result) then
    RaiseConvertError(IntToStr(Year) + '-' + IntToStr(Month) + '-' + IntToStr(Day), 'date');
end;

function EncodeTime(Hour, Min, Sec, MSec: Word): TDateTime;
begin
  if not TryEncodeTime(Hour, Min, Sec, MSec, Result) then
    RaiseConvertError(IntToStr(Hour) + ':' + IntToStr(Min) + ':' + IntToStr(Sec) + '.' +
      IntToStr(MSec), 'time');
end;

procedure DecodeDate(const DateTime: TDateTime; out Year, Month, Day: Word);
begin
  DtCivilFromDays(DtFloorDiv(DtToMillis(DateTime), MSecsPerDay), Year, Month, Day);
end;

procedure DecodeTime(const DateTime: TDateTime; out Hour, Min, Sec, MSec: Word);
begin
  var Millis := DtToMillis(DateTime);
  var InDay := Millis - DtFloorDiv(Millis, MSecsPerDay) * MSecsPerDay;
  Hour := InDay div 3600000;
  Min := InDay div 60000 mod 60;
  Sec := InDay div 1000 mod 60;
  MSec := InDay mod 1000;
end;

function DecodeDateFully(const DateTime: TDateTime; out Year, Month, Day, DOW: Word): Boolean;
begin
  DecodeDate(DateTime, Year, Month, Day);
  DOW := DayOfWeek(DateTime);
  Result := IsLeapYear(Year);
end;

procedure ReplaceDate(var DateTime: TDateTime; const NewDate: TDateTime);
begin
  var Millis := DtToMillis(DateTime);
  var InDay := Millis - DtFloorDiv(Millis, MSecsPerDay) * MSecsPerDay;
  DateTime := DtFromMillis(DtFloorDiv(DtToMillis(NewDate), MSecsPerDay) * MSecsPerDay + InDay);
end;

procedure ReplaceTime(var DateTime: TDateTime; const NewTime: TDateTime);
begin
  var Millis := DtToMillis(NewTime);
  var InDay := Millis - DtFloorDiv(Millis, MSecsPerDay) * MSecsPerDay;
  DateTime := DtFromMillis(DtFloorDiv(DtToMillis(DateTime), MSecsPerDay) * MSecsPerDay + InDay);
end;

function IncMonth(const DateTime: TDateTime; NumberOfMonths: Integer): TDateTime;
var
  Year, Month, Day: Word;
begin
  var Millis := DtToMillis(DateTime);
  var Days := DtFloorDiv(Millis, MSecsPerDay);
  DtCivilFromDays(Days, Year, Month, Day);
  var Months: Int64 := Int64(Year) * 12 + (Month - 1) + NumberOfMonths;
  var NewYear := DtFloorDiv(Months, 12);
  var NewMonth := Months - NewYear * 12 + 1;
  if (NewYear < 1) or (NewYear > 9999) then
    raise ERangeError.Create('IncMonth goes out of the years 1 to 9999');
  var NewDay: Int64 := Day;
  if NewDay > DtDaysInMonth(NewYear, NewMonth) then
    NewDay := DtDaysInMonth(NewYear, NewMonth);
  Result := DtFromMillis(DtDaysFromCivil(NewYear, NewMonth, NewDay) * MSecsPerDay +
    (Millis - Days * MSecsPerDay));
end;

// Whether the format, outside its quoted text, has an AM/PM specifier (the hours are then
// 1 to 12).
function DtHasAmPm(const Format: string): Boolean;
begin
  var I := 1;
  var U := UpperCase(Format);
  while I <= Length(U) do
  begin
    if (U[I] = '''') or (U[I] = '"') then
    begin
      var Q := U[I];
      Inc(I);
      while (I <= Length(U)) and (U[I] <> Q) do
        Inc(I);
    end
    else if (U[I] = 'A') and ((Copy(U, I, 5) = 'AM/PM') or (Copy(U, I, 3) = 'A/P') or
      (Copy(U, I, 4) = 'AMPM')) then
      Exit(True);
    Inc(I);
  end;
  Result := False;
end;

// The text of Format with the parts of Millis (from 1899-12-30) in place of its specifiers.
function DtFormat(const Format: string; const Millis: Int64): string;
var
  Year, Month, Day: Word;
begin
  var Days := DtFloorDiv(Millis, MSecsPerDay);
  var InDay := Millis - Days * MSecsPerDay;
  DtCivilFromDays(Days, Year, Month, Day);
  var DOW := (Days mod 7 + 13) mod 7 + 1;
  var Hour := InDay div 3600000;
  var Minute := InDay div 60000 mod 60;
  var Second := InDay div 1000 mod 60;
  var MilliSecond := InDay mod 1000;
  var Clock12 := DtHasAmPm(Format);
  var AfterHour := False;
  Result := '';
  var I := 1;
  while I <= Length(Format) do
  begin
    var C := UpCase(Format[I]);
    // The length of the run of the same letter
    var N := 1;
    while (I + N <= Length(Format)) and (UpCase(Format[I + N]) = C) do
      Inc(N);
    case C of
      '''', '"':
        begin
          var Close := I + 1;
          while (Close <= Length(Format)) and (Format[Close] <> Format[I]) do
            Inc(Close);
          Result := Result + Copy(Format, I + 1, Close - I - 1);
          I := Close + 1;
          Continue;
        end;
      'C':
        begin
          Result := Result + DtFormat(DtShortDateFormat, Millis);
          if InDay <> 0 then
            Result := Result + ' ' + DtFormat(DtLongTimeFormat, Millis);
          N := 1;
        end;
      'D':
        case N of
          1: Result := Result + IntToStr(Day);
          2: Result := Result + DtPad(Day, 2);
          3: Result := Result + DtDayName(DOW, False);
          4: Result := Result + DtDayName(DOW, True);
          5: Result := Result + DtFormat(DtShortDateFormat, Millis);
        else
          N := 6;
          Result := Result + DtFormat(DtLongDateFormat, Millis);
        end;
      'M':
        if AfterHour then
        begin
          if N = 1 then
            Result := Result + IntToStr(Minute)
          else
          begin
            N := 2;
            Result := Result + DtPad(Minute, 2);
          end;
        end
        else
          case N of
            1: Result := Result + IntToStr(Month);
            2: Result := Result + DtPad(Month, 2);
            3: Result := Result + DtMonthName(Month, False);
          else
            N := 4;
            Result := Result + DtMonthName(Month, True);
          end;
      'Y':
        // Two digits up to `yy`; three or more letters, the whole run, four digits
        if N <= 2 then
          Result := Result + DtPad(Year mod 100, 2)
        else
          Result := Result + DtPad(Year, 4);
      'H':
        begin
          var H := Hour;
          if Clock12 then
          begin
            H := Hour mod 12;
            if H = 0 then
              H := 12;
          end;
          if N = 1 then
            Result := Result + IntToStr(H)
          else
          begin
            N := 2;
            Result := Result + DtPad(H, 2);
          end;
        end;
      'N':
        if N = 1 then
          Result := Result + IntToStr(Minute)
        else
        begin
          N := 2;
          Result := Result + DtPad(Minute, 2);
        end;
      'S':
        if N = 1 then
          Result := Result + IntToStr(Second)
        else
        begin
          N := 2;
          Result := Result + DtPad(Second, 2);
        end;
      'Z':
        // `z` and `zz`: one specifier, without padding
        if N < 3 then
          Result := Result + IntToStr(MilliSecond)
        else
        begin
          N := 3;
          Result := Result + DtPad(MilliSecond, 3);
        end;
      'T':
        if N = 1 then
          Result := Result + DtFormat(DtShortTimeFormat, Millis)
        else
        begin
          N := 2;
          Result := Result + DtFormat(DtLongTimeFormat, Millis);
        end;
      'A':
        begin
          N := 1;
          var U := UpperCase(Copy(Format, I, 5));
          if U = 'AM/PM' then
          begin
            N := 5;
            if Hour < 12 then
              Result := Result + Copy(Format, I, 2)
            else
              Result := Result + Copy(Format, I + 3, 2);
          end
          else if Copy(U, 1, 3) = 'A/P' then
          begin
            N := 3;
            if Hour < 12 then
              Result := Result + Format[I]
            else
              Result := Result + Format[I + 2];
          end
          else if Copy(U, 1, 4) = 'AMPM' then
          begin
            N := 4;
            if Hour < 12 then
              Result := Result + 'AM'
            else
              Result := Result + 'PM';
          end
          else
            Result := Result + Format[I];
        end;
      '/':
        begin
          N := 1;
          Result := Result + DtDateSeparator;
        end;
      ':':
        begin
          N := 1;
          Result := Result + DtTimeSeparator;
        end;
    else
      begin
        N := 1;
        Result := Result + Format[I];
      end;
    end;
    // `m` right after the hours is the minutes
    if C = 'H' then
      AfterHour := True
    else if C in ['D', 'M', 'Y', 'N', 'S', 'Z', 'T', 'C'] then
      AfterHour := False;
    I := I + N;
  end;
end;

function FormatDateTime(const Format: string; const DateTime: TDateTime): string;
begin
  Result := DtFormat(Format, DtToMillis(DateTime));
end;

function DateToStr(const DateTime: TDateTime): string;
begin
  Result := FormatDateTime(DtShortDateFormat, DateTime);
end;

function TimeToStr(const DateTime: TDateTime): string;
begin
  Result := FormatDateTime(DtLongTimeFormat, DateTime);
end;

function DateTimeToStr(const DateTime: TDateTime): string;
begin
  Result := FormatDateTime('c', DateTime);
end;

// The number of 1 to MaxDigits digits of S from I, I moved past them.
function DtReadNumber(const S: string; var I: Integer; MaxDigits: Integer; out Value: Integer): Boolean;
begin
  Value := 0;
  var Start := I;
  while (I <= Length(S)) and (I - Start < MaxDigits) and (S[I] >= '0') and (S[I] <= '9') do
  begin
    Value := Value * 10 + (Ord(S[I]) - Ord('0'));
    Inc(I);
  end;
  Result := I > Start;
end;

// A date `Y-M-D` (a year of 1 to 4 digits, a month and a day of 1 or 2) from I, as the days
// from 1899-12-30.
function DtReadDate(const S: string; var I: Integer; out Days: Int64): Boolean;
var
  Y, M, D: Integer;
begin
  Days := 0;
  M := 0;
  D := 0;
  Result := DtReadNumber(S, I, 4, Y) and (I <= Length(S)) and (S[I] = DtDateSeparator);
  if Result then
  begin
    Inc(I);
    Result := DtReadNumber(S, I, 2, M) and (I <= Length(S)) and (S[I] = DtDateSeparator);
  end;
  if Result then
  begin
    Inc(I);
    Result := DtReadNumber(S, I, 2, D) and (Y >= 1) and (M >= 1) and (M <= 12) and (D >= 1) and
      (D <= DtDaysInMonth(Y, M));
  end;
  if Result then
    Days := DtDaysFromCivil(Y, M, D);
end;

// A time `h:n[:s[.z]]` (1 or 2 digits each, 1 to 3 of the milliseconds) from I, as the
// milliseconds of the day.
function DtReadTime(const S: string; var I: Integer; out Millis: Int64): Boolean;
var
  H, N, Sec, Z: Integer;
begin
  Millis := 0;
  N := 0;
  Sec := 0;
  Z := 0;
  Result := DtReadNumber(S, I, 2, H) and (I <= Length(S)) and (S[I] = DtTimeSeparator);
  if Result then
  begin
    Inc(I);
    Result := DtReadNumber(S, I, 2, N);
  end;
  if Result and (I <= Length(S)) and (S[I] = DtTimeSeparator) then
  begin
    Inc(I);
    Result := DtReadNumber(S, I, 2, Sec);
    if Result and (I <= Length(S)) and (S[I] = '.') then
    begin
      Inc(I);
      var Start := I;
      Result := DtReadNumber(S, I, 3, Z);
      for var K := I - Start + 1 to 3 do
        Z := Z * 10;
    end;
  end;
  Result := Result and (H < HoursPerDay) and (N < MinsPerHour) and (Sec < SecsPerMin);
  if Result then
    Millis := ((H * 60 + N) * 60 + Sec) * 1000 + Z;
end;

function TryStrToDate(const S: string; out Value: TDateTime): Boolean;
var
  Days: Int64;
begin
  Value := 0;
  var T := Trim(S);
  var I := 1;
  Result := DtReadDate(T, I, Days) and (I > Length(T));
  if Result then
    Value := Days;
end;

function TryStrToTime(const S: string; out Value: TDateTime): Boolean;
var
  Millis: Int64;
begin
  Value := 0;
  var T := Trim(S);
  var I := 1;
  Result := DtReadTime(T, I, Millis) and (I > Length(T));
  if Result then
    Value := Millis / MSecsPerDay;
end;

function TryStrToDateTime(const S: string; out Value: TDateTime): Boolean;
var
  Days, Millis: Int64;
begin
  Value := 0;
  var T := Trim(S);
  var I := 1;
  Millis := 0;
  Result := DtReadDate(T, I, Days);
  if Result and (I <= Length(T)) then
  begin
    Result := T[I] = ' ';
    while (I <= Length(T)) and (T[I] = ' ') do
      Inc(I);
    Result := Result and DtReadTime(T, I, Millis);
  end;
  Result := Result and (I > Length(T));
  if Result then
    Value := DtFromMillis(Days * MSecsPerDay + Millis);
end;

function StrToDate(const S: string): TDateTime;
begin
  if not TryStrToDate(S, Result) then
    RaiseConvertError(S, 'date');
end;

function StrToTime(const S: string): TDateTime;
begin
  if not TryStrToTime(S, Result) then
    RaiseConvertError(S, 'time');
end;

function StrToDateTime(const S: string): TDateTime;
begin
  if not TryStrToDateTime(S, Result) then
    RaiseConvertError(S, 'date and time');
end;

procedure Sleep(Milliseconds: Cardinal);
const
  // The status of ClockSleep past the range of the virtual clock (docs/spec/bytecode.md 2.22)
  SlOutOfRange = 2;
begin
  if ClockSleep(Int64(Milliseconds) * 1000000) = SlOutOfRange then
    raise ERangeError.Create('Sleep(' + IntToStr(Milliseconds) +
      ') would move the virtual clock past its range');
end;
