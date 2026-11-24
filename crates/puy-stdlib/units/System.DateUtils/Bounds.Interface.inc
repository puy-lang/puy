// The start (midnight) and the end (23:59:59.999) of the day, the ISO 8601 week (Monday to
// Sunday), the month and the year of a value, or of the year and month given; ERangeError when
// it is outside the years 1 to 9999.

function StartOfTheDay(const AValue: TDateTime): TDateTime;
function EndOfTheDay(const AValue: TDateTime): TDateTime;
function StartOfTheWeek(const AValue: TDateTime): TDateTime;
function EndOfTheWeek(const AValue: TDateTime): TDateTime;
function StartOfTheMonth(const AValue: TDateTime): TDateTime;
function EndOfTheMonth(const AValue: TDateTime): TDateTime;
function StartOfTheYear(const AValue: TDateTime): TDateTime;
function EndOfTheYear(const AValue: TDateTime): TDateTime;
function StartOfAMonth(const AYear, AMonth: Word): TDateTime;
function EndOfAMonth(const AYear, AMonth: Word): TDateTime;
function StartOfAYear(const AYear: Word): TDateTime;
function EndOfAYear(const AYear: Word): TDateTime;
