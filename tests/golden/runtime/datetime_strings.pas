program DateTimeStrings;

// The fixed formats of System.SysUtils (requirements §5.11): DateToStr, TimeToStr and
// DateTimeToStr, the English names of days and months (Delphi's follow the OS's locale), the
// formats `c`, `ddddd`, `dddddd`, `t`, `tt` (the fixed formats, in 24 hours even beside am/pm)
// and the separators `/` and `:`, `yyy` and `zz` as one specifier, the range of the years 1 to
// 9999, the forms StrToDate / StrToTime / StrToDateTime read, and their errors.

uses
  System.SysUtils;

procedure TryAll(const S: string);
var
  D: TDateTime;
begin
  Write('[', S, ']');
  if TryStrToDate(S, D) then
    Write(' date ', DateTimeToStr(D));
  if TryStrToTime(S, D) then
    Write(' time ', FormatDateTime('hh:nn:ss.zzz', D));
  if TryStrToDateTime(S, D) then
    Write(' datetime ', FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', D));
  WriteLn;
end;

procedure Check(const What: string; const P: TProc);
begin
  try
    P();
    WriteLn(What, ': no exception');
  except
    on E: Exception do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

begin
  var D := EncodeDate(2026, 3, 7) + EncodeTime(14, 5, 9, 45);
  WriteLn(DateToStr(D), ' | ', TimeToStr(D), ' | ', DateTimeToStr(D));
  WriteLn(DateTimeToStr(EncodeDate(2026, 3, 7)), ' | ', TimeToStr(EncodeDate(2026, 3, 7)));
  WriteLn(FormatDateTime('c', D), ' | ', FormatDateTime('ddddd', D), ' | ',
    FormatDateTime('dddddd', D));
  WriteLn(FormatDateTime('ddd dddd mmm mmmm', D));
  WriteLn(FormatDateTime('t', D), ' | ', FormatDateTime('tt', D), ' | ',
    FormatDateTime('yyyy/mm/dd hh:nn', D));
  WriteLn(DateTimeToStr(EncodeDate(1, 1, 1)), ' | ', DateToStr(EncodeDate(999, 12, 31)));
  WriteLn(FormatDateTime('yyy-mm-dd zz|zzz|z', D), ' | ', FormatDateTime('t am/pm', D + 0.0));
  // The first day of the range, its noon, and the day before it (the year 0) out of the range
  WriteLn(DateTimeToStr(-693593), ' | ', DateTimeToStr(-693593.5));
  // Before 1899-12-30 the fraction counts away from zero
  WriteLn(DateTimeToStr(-1.25), ' | ', DateTimeToStr(-0.5), ' | ', DateTimeToStr(0.5));

  TryAll('2026-03-07');
  TryAll(' 2026-3-7 ');
  TryAll('14:05');
  TryAll('14:05:09');
  TryAll('14:05:09.4');
  TryAll('2026-03-07 14:05:09.045');
  TryAll('2026-03-07  4:05');
  TryAll('2026-02-29');
  TryAll('2026-03-07T14:05:09');
  TryAll('24:00');
  TryAll('2026/03/07');
  TryAll('');

  Check('StrToDate', procedure begin StrToDate('2026-02-30') end);
  Check('StrToTime', procedure begin StrToTime('25:00') end);
  Check('StrToDateTime', procedure begin StrToDateTime('tomorrow') end);
  Check('EncodeDate', procedure begin EncodeDate(2026, 2, 29) end);
  Check('EncodeTime', procedure begin EncodeTime(24, 0, 0, 0) end);
  Check('DecodeDate', procedure
    var
      Y, M, Day: Word;
    begin
      DecodeDate(3000000, Y, M, Day);
      WriteLn(Y, M, Day);
    end);
  Check('FormatDateTime', procedure begin FormatDateTime('yyyy', -700000) end);
  Check('year 0', procedure begin DateTimeToStr(-693594) end);
  Check('year 0 noon', procedure begin DateTimeToStr(-693594.5) end);
  Check('IncMonth', procedure begin IncMonth(EncodeDate(9999, 12, 1)) end);
end.
