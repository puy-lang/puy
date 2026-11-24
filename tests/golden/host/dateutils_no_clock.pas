// puy-test: no-clock
program DateUtilsNoClock;

// Without a clock, the routines that ask for the local time zone raise ENotSupportedException;
// the ones on UTC values, and on ISO 8601 text without an offset, do not ask.

uses
  System.SysUtils, System.DateUtils;

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

begin
  WriteLn(DateToISO8601(EncodeDate(2026, 1, 2)), ' ', DateTimeToUnix(EncodeDate(2026, 1, 2)), ' ',
    DateToISO8601(ISO8601ToDate('2026-01-02T03:04:05', False)));
  Fails('DateToISO8601', procedure begin DateToISO8601(EncodeDate(2026, 1, 2), False) end);
  Fails('DateTimeToUnix', procedure begin DateTimeToUnix(EncodeDate(2026, 1, 2), False) end);
  Fails('UnixToDateTime', procedure begin UnixToDateTime(0, False) end);
  Fails('ISO8601ToDate', procedure begin ISO8601ToDate('2026-01-02T03:04:05Z', False) end);
  Fails('Today', procedure begin Today end);
end.
