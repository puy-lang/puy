// ISO 8601 text and UNIX time. A TDateTime has no time zone: AInputIsUTC and AReturnUTC say
// whether it is UTC or the local time (the time zone of TZ, or the OS's; UTC where the host
// cannot tell, as System.SysUtils's Now).

// yyyy-mm-ddThh:nn:ss.zzz followed by Z (UTC) or the offset of the local time (+09:00).
function DateToISO8601(const ADate: TDateTime; AInputIsUTC: Boolean = True): string;
// YYYY-MM-DD, optionally followed by T (t or a space) and hh:mm, hh:mm:ss or hh:mm:ss.fff (any
// digits of the fraction, cut to milliseconds), and after the time optionally Z (z), +hh:mm,
// +hhmm or +hh (- too). With an offset the value is UTC, or the local time when not AReturnUTC;
// without one it is the time as written. EConvertError when the text is not one of these, or the
// result is outside the years 1 to 9999.
function ISO8601ToDate(const AISODate: string; AReturnUTC: Boolean = True): TDateTime;
function TryISO8601ToDate(const AISODate: string; out Value: TDateTime;
  AReturnUTC: Boolean = True): Boolean;

// The seconds since 1970-01-01T00:00:00Z (toward minus infinity).
function DateTimeToUnix(const AValue: TDateTime; AInputIsUTC: Boolean = True): Int64;
// ERangeError when the result is outside the years 1 to 9999.
function UnixToDateTime(const AValue: Int64; AReturnUTC: Boolean = True): TDateTime;
