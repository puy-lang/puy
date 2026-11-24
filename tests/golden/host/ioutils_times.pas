// puy-test: test-fs, clock=1795165200000000000, tz-offset=3600, tz-change=1774746000000000000:7200, tz-change=1792890000000000000:3600
program IOUtilsTimes;

// The times of TFile in the local time and as TInstant (spec/stdlib/System.IOUtils/TFile.md 4)
// in a zone of UTC+1 with daylight saving time (UTC+2) from 2026-03-29T01:00Z to
// 2026-10-25T01:00Z: reading takes the offset of the instant; setting a local time that occurs
// twice (autumn) is the earlier instant, and one that is skipped (spring) is read with the offset
// before the change. The TInstant versions keep the nanoseconds; TFile's work on a directory.

uses
  System.SysUtils,
  System.IOUtils;

function Show(const T: TDateTime): string;
begin
  Result := FormatDateTime('yyyy-mm-dd hh:nn:ss.zzz', T);
end;

procedure ShowAll(const What, Path: string);
begin
  WriteLn(What, ': local ', Show(TFile.GetLastWriteTime(Path)), ' | utc ',
    Show(TFile.GetLastWriteTimeUtc(Path)), ' | ', TFile.GetLastWriteInstant(Path).ToString);
end;

begin
  TFile.WriteAllText('a.txt', 'x');
  // The runner's file system starts at 2026-01-02T03:04:05Z (winter, UTC+1)
  ShowAll('initial', 'a.txt');

  // TInstant keeps the nanoseconds; the local time of a summer instant is UTC+2
  TFile.SetLastWriteInstant('a.txt', TInstant.Parse('2026-06-01T12:00:00.123456789Z'));
  ShowAll('instant', 'a.txt');
  TFile.SetLastAccessInstant('a.txt', TInstant.FromUnixNanos(-1));
  WriteLn('access instant: ', TFile.GetLastAccessInstant('a.txt').ToString);
  TFile.SetCreationInstant('a.txt', TInstant.FromUnixSeconds(0));
  WriteLn('creation instant: ', TFile.GetCreationInstant('a.txt').ToString, ' | local ',
    Show(TFile.GetCreationTime('a.txt')));

  // Local times in winter and in summer
  TFile.SetLastWriteTime('a.txt', EncodeDate(2026, 12, 1) + EncodeTime(10, 0, 0, 0));
  ShowAll('winter', 'a.txt');
  TFile.SetLastWriteTime('a.txt', EncodeDate(2026, 7, 1) + EncodeTime(10, 0, 0, 0));
  ShowAll('summer', 'a.txt');
  TFile.SetLastAccessTime('a.txt', EncodeDate(2026, 7, 2) + EncodeTime(11, 30, 0, 250));
  WriteLn('access: local ', Show(TFile.GetLastAccessTime('a.txt')), ' | utc ',
    Show(TFile.GetLastAccessTimeUtc('a.txt')));
  TFile.SetCreationTime('a.txt', EncodeDate(2026, 2, 3) + EncodeTime(4, 5, 6, 0));
  WriteLn('creation: local ', Show(TFile.GetCreationTime('a.txt')), ' | utc ',
    Show(TFile.GetCreationTimeUtc('a.txt')));

  // Around the changes: the last local time before each, the times that occur twice or not at
  // all, and the first after
  TFile.SetLastWriteTime('a.txt', EncodeDate(2026, 3, 29) + EncodeTime(1, 59, 59, 999));
  ShowAll('before spring', 'a.txt');
  TFile.SetLastWriteTime('a.txt', EncodeDate(2026, 3, 29) + EncodeTime(2, 30, 0, 0));
  ShowAll('skipped', 'a.txt');
  TFile.SetLastWriteTime('a.txt', EncodeDate(2026, 3, 29) + EncodeTime(3, 0, 0, 0));
  ShowAll('after spring', 'a.txt');
  TFile.SetLastWriteTime('a.txt', EncodeDate(2026, 10, 25) + EncodeTime(1, 59, 59, 999));
  ShowAll('before autumn', 'a.txt');
  TFile.SetLastWriteTime('a.txt', EncodeDate(2026, 10, 25) + EncodeTime(2, 30, 0, 0));
  ShowAll('twice', 'a.txt');
  TFile.SetLastWriteTime('a.txt', EncodeDate(2026, 10, 25) + EncodeTime(3, 0, 0, 0));
  ShowAll('after autumn', 'a.txt');
  // The second 02:30 of the autumn can be set in UTC or as an instant
  TFile.SetLastWriteTimeUtc('a.txt', EncodeDate(2026, 10, 25) + EncodeTime(1, 30, 0, 0));
  ShowAll('second 02:30', 'a.txt');

  // A directory
  TDirectory.CreateDirectory('d');
  TFile.SetLastWriteTime('d', EncodeDate(2026, 8, 9) + EncodeTime(10, 11, 12, 0));
  ShowAll('directory', 'd');

  // Out of the range of file times, as the UTC versions
  try
    TFile.SetLastWriteTime('a.txt', 1e9);
  except
    on E: Exception do
      WriteLn('out of range: ', E.ClassName, ': ', E.Message);
  end;
  // Reading a time whose local time is past the range of TInstant
  TFile.SetLastWriteInstant('a.txt', TInstant.FromUnixNanos(High(Int64)));
  WriteLn('last instant: ', TFile.GetLastWriteInstant('a.txt').ToString);
  try
    TFile.GetLastWriteTime('a.txt');
  except
    on E: Exception do
      WriteLn('local out of range: ', E.ClassName, ': ', E.Message);
  end;
  try
    TFile.GetLastWriteInstant('missing.txt');
  except
    on E: Exception do
      WriteLn('missing: ', E.ClassName, ': ', E.Message);
  end;
end.
