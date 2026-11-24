// puy-test: test-fs, no-clock
program IOUtilsTimesNoClock;

// Without the host's clock there is no time zone: the local versions of TFile's times raise
// System.SysUtils's exception of a missing clock, while the UTC and TInstant versions still work
// (spec/stdlib/System.IOUtils/TFile.md 4).

uses
  System.SysUtils,
  System.IOUtils;

begin
  TFile.WriteAllText('a.txt', 'x');
  WriteLn('utc: ', FormatDateTime('yyyy-mm-dd hh:nn:ss', TFile.GetLastWriteTimeUtc('a.txt')));
  TFile.SetLastWriteInstant('a.txt', TInstant.FromUnixSeconds(86400));
  WriteLn('instant: ', TFile.GetLastWriteInstant('a.txt').ToString);
  try
    TFile.GetLastWriteTime('a.txt');
  except
    on E: ENotSupportedException do
      WriteLn('local: ', E.ClassName, ' ', E.ErrorKind, ': ', E.Message);
  end;
  try
    TFile.SetLastWriteTime('a.txt', 0);
  except
    on E: ENotSupportedException do
      WriteLn('set local: ', E.ClassName, ' ', E.ErrorKind, ': ', E.Message);
  end;
  WriteLn('unchanged: ', TFile.GetLastWriteInstant('a.txt').ToString);
end.
