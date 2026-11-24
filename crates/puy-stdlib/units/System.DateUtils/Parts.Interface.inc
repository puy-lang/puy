// The parts of a date and time, validity, today, and comparisons.

// The parts of a date and time, rounded to the millisecond as DecodeDate and DecodeTime do;
// ERangeError outside the years 1 to 9999.
function YearOf(const AValue: TDateTime): Word;
function MonthOf(const AValue: TDateTime): Word;
function DayOf(const AValue: TDateTime): Word;
function HourOf(const AValue: TDateTime): Word;
function MinuteOf(const AValue: TDateTime): Word;
function SecondOf(const AValue: TDateTime): Word;
function MilliSecondOf(const AValue: TDateTime): Word;
procedure DecodeDateTime(const AValue: TDateTime; out AYear, AMonth, ADay, AHour, AMinute,
  ASecond, AMilliSecond: Word);
// EConvertError when a part is not valid (as EncodeDate and EncodeTime).
function EncodeDateTime(const AYear, AMonth, ADay, AHour, AMinute, ASecond,
  AMilliSecond: Word): TDateTime;
function TryEncodeDateTime(const AYear, AMonth, ADay, AHour, AMinute, ASecond,
  AMilliSecond: Word; out AValue: TDateTime): Boolean;

// The day of the year (1 to 366), and the day of the week by ISO 8601 (1, Monday, to 7).
function DayOfTheYear(const AValue: TDateTime): Word;
function DayOfTheWeek(const AValue: TDateTime): Word;
// The week of the year by ISO 8601 (1 to 53; the week of the Thursday, starting on Monday).
// AYear is the year the week belongs to, which differs from the year of the date at its ends.
function WeekOfTheYear(const AValue: TDateTime): Word; overload;
function WeekOfTheYear(const AValue: TDateTime; out AYear: Word): Word; overload;
function WeekOf(const AValue: TDateTime): Word;
function DaysInMonth(const AValue: TDateTime): Word;
function DaysInYear(const AValue: TDateTime): Word;
function DaysInAMonth(const AYear, AMonth: Word): Word;
function DaysInAYear(const AYear: Word): Word;
// The ISO 8601 weeks of a year (52 or 53).
function WeeksInYear(const AValue: TDateTime): Word;
function WeeksInAYear(const AYear: Word): Word;
function IsInLeapYear(const AValue: TDateTime): Boolean;
// Whether the time is noon or later.
function IsPM(const AValue: TDateTime): Boolean;

function IsValidDate(const AYear, AMonth, ADay: Word): Boolean;
function IsValidTime(const AHour, AMinute, ASecond, AMilliSecond: Word): Boolean;
function IsValidDateTime(const AYear, AMonth, ADay, AHour, AMinute, ASecond,
  AMilliSecond: Word): Boolean;

// The date at midnight, and the time with the date 1899-12-30.
function DateOf(const AValue: TDateTime): TDateTime;
function TimeOf(const AValue: TDateTime): TDateTime;
// The local date at midnight (Date), the day before it and the day after it.
function Today: TDateTime;
function Yesterday: TDateTime;
function Tomorrow: TDateTime;
function IsToday(const AValue: TDateTime): Boolean;
function IsSameDay(const AValue, ABasis: TDateTime): Boolean;

// -1, 0 or 1 by the milliseconds: of the whole values, of the dates only, of the times only.
function CompareDateTime(const A, B: TDateTime): Integer;
function CompareDate(const A, B: TDateTime): Integer;
function CompareTime(const A, B: TDateTime): Integer;
function SameDateTime(const A, B: TDateTime): Boolean;
function SameDate(const A, B: TDateTime): Boolean;
function SameTime(const A, B: TDateTime): Boolean;
