program DateUtilsIso8601;

// ISO 8601 text of UTC values, with Z and with offsets (the other forms are in dateutils_edge).
// Kept out of diff/ because Delphi XE4 does not have DateToISO8601 and ISO8601ToDate.

uses
  System.SysUtils, System.DateUtils;

procedure Show(const D: TDateTime);
begin
  WriteLn(FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', D));
end;

begin
  WriteLn(DateToISO8601(EncodeDateTime(2026, 11, 20, 12, 34, 56, 789)));
  WriteLn(DateToISO8601(EncodeDate(2000, 2, 29)));
  Show(ISO8601ToDate('2026-11-20T12:34:56.789Z'));
  Show(ISO8601ToDate('2026-11-20T12:34:56Z'));
  Show(ISO8601ToDate('2026-11-20T21:34:56+09:00'));
  Show(ISO8601ToDate('2026-11-20T07:04:56-05:30'));
  Show(ISO8601ToDate('2026-01-01T03:00:00+05:00'));
end.
