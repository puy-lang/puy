// puy-test: no-fs
program IOUtilsNoFs;

// Without `Fs` and `Os` (spec/stdlib/System.IOUtils/README.md): TFile and TDirectory raise
// ENotSupportedException with ErrorKind ekNoPermission (Exists too, as FileExists), not the
// exceptions of a missing file; TPath still works, and the directories of the OS are ''.

uses
  System.SysUtils, System.IOUtils;

procedure Attempt(const What: string; Op: Integer);
begin
  try
    case Op of
      0: TFile.Exists('a.txt');
      1: TFile.ReadAllText('a.txt');
      2: TFile.WriteAllText('a.txt', 'x');
      3: TFile.Delete('a.txt');
      4: TFile.GetLastWriteTimeUtc('a.txt');
      5: TDirectory.GetFiles('.');
      6: TDirectory.CreateDirectory('d');
      7: TDirectory.Delete('d');
      8: TFile.Copy('a.txt', 'b.txt');
    end;
    WriteLn(What, ': no exception');
  except
    on E: ENotSupportedException do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message, ' (', E.ErrorKind, ')');
  end;
end;

begin
  Attempt('TFile.Exists', 0);
  Attempt('ReadAllText', 1);
  Attempt('WriteAllText', 2);
  Attempt('TFile.Delete', 3);
  Attempt('GetLastWriteTimeUtc', 4);
  Attempt('GetFiles', 5);
  Attempt('CreateDirectory', 6);
  Attempt('TDirectory.Delete', 7);
  Attempt('TFile.Copy', 8);
  WriteLn('[', TPath.GetTempPath, '] [', TPath.GetHomePath, '] ', TPath.Combine('a', 'b'));
end.
