// Dates and times as TDateTime (docs/spec/stdlib/System.SysUtils/DateTime.md): the days from
// 1899-12-30 with the time of day as the fraction. The formats are fixed whatever the OS's
// locale (requirements §5.11): dates yyyy-mm-dd, times hh:nn:ss, English names.

const
  HoursPerDay = 24;
  MinsPerHour = 60;
  SecsPerMin = 60;
  MSecsPerSec = 1000;
  MinsPerDay = 1440;
  SecsPerDay = 86400;
  MSecsPerDay = 86400000;
  // The days from 0000-12-31 to 1899-12-30 (0001-01-01 is day 1), and from 1899-12-30 to
  // 1970-01-01
  DateDelta = 693594;
  UnixDateDelta = 25569;

// The local date and time of the host's clock (the time zone of TZ, or the OS's; UTC where the
// host cannot tell), to the millisecond. Without a clock, ENotSupportedException.
function Now: TDateTime;
// Now without the time, and without the date.
function Date: TDateTime;
function Time: TDateTime;

function IsLeapYear(Year: Word): Boolean;
// The day of the week, 1 (Sunday) to 7 (Saturday).
function DayOfWeek(const DateTime: TDateTime): Word;

// A date of the years 1 to 9999, and a time of the day; EConvertError when one is not valid.
function EncodeDate(Year, Month, Day: Word): TDateTime;
function EncodeTime(Hour, Min, Sec, MSec: Word): TDateTime;
function TryEncodeDate(Year, Month, Day: Word; out Date: TDateTime): Boolean;
function TryEncodeTime(Hour, Min, Sec, MSec: Word; out Time: TDateTime): Boolean;
// The parts of a date and time, rounded to the millisecond; ERangeError outside the years 1 to
// 9999.
procedure DecodeDate(const DateTime: TDateTime; out Year, Month, Day: Word);
procedure DecodeTime(const DateTime: TDateTime; out Hour, Min, Sec, MSec: Word);
function DecodeDateFully(const DateTime: TDateTime; out Year, Month, Day, DOW: Word): Boolean;
procedure ReplaceDate(var DateTime: TDateTime; const NewDate: TDateTime);
procedure ReplaceTime(var DateTime: TDateTime; const NewTime: TDateTime);
// The date NumberOfMonths months later (earlier when negative), the day kept within the month,
// the time kept.
function IncMonth(const DateTime: TDateTime; NumberOfMonths: Integer = 1): TDateTime;

// yyyy-mm-dd, hh:nn:ss, and both with a space between them (the date alone when the time is
// midnight).
function DateToStr(const DateTime: TDateTime): string;
function TimeToStr(const DateTime: TDateTime): string;
function DateTimeToStr(const DateTime: TDateTime): string;
function FormatDateTime(const Format: string; const DateTime: TDateTime): string;

// The forms DateToStr, TimeToStr and DateTimeToStr write (seconds and milliseconds optional);
// EConvertError when S is not one.
function StrToDate(const S: string): TDateTime;
function StrToTime(const S: string): TDateTime;
function StrToDateTime(const S: string): TDateTime;
function TryStrToDate(const S: string; out Value: TDateTime): Boolean;
function TryStrToTime(const S: string; out Value: TDateTime): Boolean;
function TryStrToDateTime(const S: string; out Value: TDateTime): Boolean;

// Waits Milliseconds on the isolate's clock (chapter 6): the host's monotonic clock, or the
// virtual clock of a test (Puy.Testing), which moves on at once.
procedure Sleep(Milliseconds: Cardinal);
