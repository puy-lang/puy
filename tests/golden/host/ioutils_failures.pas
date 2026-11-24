// puy-test: test-fs
program IOUtilsFailures;

// What Puy's TFile and TDirectory raise where it differs from Delphi XE4 or diff/ioutils_errors
// does not show it (spec/stdlib/System.IOUtils/README.md): EFileNotFoundException and
// EDirectoryNotFoundException are EInOutErrors with ErrorKind ekNotFound, an empty path is
// EArgumentException everywhere (CreateDirectory too), copying a directory into itself is
// EArgumentException, and the messages name the operation and the paths (both for Copy / Move).

uses
  System.SysUtils, System.Classes, System.IOUtils;

procedure Attempt(const What: string; Op: Integer);
begin
  try
    case Op of
      0: TFile.ReadAllText('missing.txt');
      1: TFile.WriteAllText('nodir/a.txt', 'x');
      2: TDirectory.GetFiles('nodir');
      3: TDirectory.CreateDirectory('');
      4: TFile.Delete('missing.txt');
      5: TFile.Copy('a.txt', 'b.txt');
      6: TFile.Copy('a.txt', 'nodir/b.txt');
      7: TFile.Move('a.txt', 'nodir/b.txt');
      8: TDirectory.Delete('d');
      9: TFile.ReadAllText('d');
      10: TFile.GetSize('missing.txt');
      11: TFile.ReadLines('missing.txt');
      12: TDirectory.SetCurrentDirectory('nodir');
      13: TFile.Open('a.txt', TFileMode.fmCreateNew);
      14: TFile.Open('missing.txt', TFileMode.fmTruncate);
      15: TFile.Open('missing.txt', TFileMode.fmOpen);
      16: TDirectory.Move('d', 'a.txt');
      17: TFile.ReadAllText('a.txt', nil);
      18: TPath.GetFullPath('');
      19: TFile.SetLastWriteTimeUtc('a.txt', 1e9);
      20: TDirectory.Copy('d', 'd/e/c');
      21: TFile.Copy('a.txt', 'd', True);
      22: TFile.Move('a.txt', 'd/e/');
      23: TDirectory.Copy('/work/d', 'd/');
    end;
    WriteLn(What, ': no exception');
  except
    on E: EInOutError do
      WriteLn(What, ': ', E.ClassName, ' (EInOutError, ', E.ErrorKind, '): ', E.Message);
    on E: EStreamError do
      WriteLn(What, ': ', E.ClassName, ' (EStreamError, ', E.ErrorKind, '): ', E.Message);
    on E: Exception do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
end;

begin
  TFile.WriteAllText('a.txt', 'a');
  TFile.WriteAllText('b.txt', 'b');
  TDirectory.CreateDirectory('d/e');
  Attempt('ReadAllText missing', 0);
  Attempt('WriteAllText no directory', 1);
  Attempt('GetFiles missing', 2);
  Attempt('CreateDirectory empty', 3);
  Attempt('Delete missing', 4);
  Attempt('Copy over existing', 5);
  Attempt('Copy to no directory', 6);
  Attempt('Move to no directory', 7);
  Attempt('TDirectory.Delete not empty', 8);
  Attempt('ReadAllText of a directory', 9);
  Attempt('GetSize missing', 10);
  Attempt('ReadLines missing', 11);
  Attempt('SetCurrentDirectory missing', 12);
  Attempt('Open fmCreateNew existing', 13);
  Attempt('Open fmTruncate missing', 14);
  Attempt('Open fmOpen missing', 15);
  Attempt('TDirectory.Move over a file', 16);
  Attempt('ReadAllText nil encoding', 17);
  Attempt('GetFullPath empty', 18);
  Attempt('SetLastWriteTimeUtc out of range', 19);
  Attempt('TDirectory.Copy into itself', 20);
  Attempt('Copy over a directory', 21);
  TFile.Copy('a.txt', 'd/e/x');
  Attempt('Move over an existing name', 22);
  Attempt('TDirectory.Copy into itself (same)', 23);
  WriteLn(TFile.ReadAllText('a.txt'), TFile.ReadAllText('b.txt'));
end.
