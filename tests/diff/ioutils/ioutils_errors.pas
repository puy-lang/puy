// puy-test: test-fs
program IOUtilsErrors;

// TFile, TDirectory and TPath of System.IOUtils (spec/stdlib/System.IOUtils) as Delphi's: the
// exceptions of the failures, what the writers put in a file, the lines ReadAllLines splits, the
// listings and the names TPath takes apart. Only names without separators, so that the case runs
// alike on every OS (listed paths are shown by their names); Puy runs it on the runner's file
// system in memory. Where Puy differs from Delphi XE4 (EFileNotFoundException and
// EDirectoryNotFoundException are EInOutErrors, CreateDirectory('') is EArgumentException) is in
// golden/host/ioutils_failures.

uses
  System.SysUtils, System.IOUtils;

procedure ShowLines(const What: string; const A: array of string);
var
  I: Integer;
begin
  Write(What, ': ', Length(A));
  for I := 0 to High(A) do
    Write(' [', A[I], ']');
  WriteLn;
end;

procedure Show(const What: string; E: Exception);
begin
  WriteLn(What, ': ', E.ClassName);
end;

procedure ShowNames(const What: string; const A: array of string);
var
  I: Integer;
begin
  Write(What, ':');
  for I := 0 to High(A) do
    Write(' ', ExtractFileName(A[I]));
  WriteLn;
end;

var
  S: string;
  B: TBytes;
begin
  S := '';
  try
    S := TFile.ReadAllText('m520_missing.txt');
  except
    on E: Exception do Show('ReadAllText missing', E);
  end;
  try
    B := TFile.ReadAllBytes('m520_missing.txt');
  except
    on E: Exception do Show('ReadAllBytes missing', E);
  end;
  try
    ShowLines('ReadAllLines missing', TFile.ReadAllLines('m520_missing.txt'));
  except
    on E: Exception do Show('ReadAllLines missing', E);
  end;
  try
    S := TFile.ReadAllText('');
  except
    on E: Exception do Show('ReadAllText empty', E);
  end;
  try
    TFile.Delete('m520_missing.txt');
    WriteLn('Delete missing: no exception');
  except
    on E: Exception do Show('Delete missing', E);
  end;
  try
    TFile.Copy('m520_missing.txt', 'm520_copy.txt');
  except
    on E: Exception do Show('Copy missing', E);
  end;
  try
    TFile.Move('m520_missing.txt', 'm520_moved.txt');
  except
    on E: Exception do Show('Move missing', E);
  end;
  try
    TDirectory.Delete('m520_nodir');
  except
    on E: Exception do Show('TDirectory.Delete missing', E);
  end;
  try
    ShowNames('GetFiles missing', TDirectory.GetFiles('m520_nodir'));
  except
    on E: Exception do Show('GetFiles missing', E);
  end;
  try
    TFile.WriteAllText('m520_nodir/a.txt', 'x');
  except
    on E: Exception do Show('WriteAllText no directory', E);
  end;
  WriteLn(TFile.Exists(''), ' ', TDirectory.Exists(''));

  TFile.WriteAllText('m520_a.txt', 'abc');
  WriteLn('WriteAllText: ', Length(TFile.ReadAllBytes('m520_a.txt')));
  TFile.WriteAllText('m520_b.txt', 'abc', TEncoding.UTF8);
  WriteLn('WriteAllText UTF8: ', Length(TFile.ReadAllBytes('m520_b.txt')));
  WriteLn('ReadAllText UTF8 BOM: ', TFile.ReadAllText('m520_b.txt'), ' ',
    Length(TFile.ReadAllText('m520_b.txt')));
  TFile.AppendAllText('m520_a.txt', 'de');
  TFile.AppendAllText('m520_c.txt', 'new');
  WriteLn('AppendAllText: ', TFile.ReadAllText('m520_a.txt'), ' ', TFile.ReadAllText('m520_c.txt'));
  try
    TFile.Copy('m520_a.txt', 'm520_c.txt');
  except
    on E: Exception do Show('Copy over existing', E);
  end;
  TFile.Copy('m520_a.txt', 'm520_c.txt', True);
  WriteLn('Copy overwrite: ', TFile.ReadAllText('m520_c.txt'));
  try
    TFile.Move('m520_a.txt', 'm520_c.txt');
  except
    on E: Exception do Show('Move over existing', E);
  end;
  TFile.Move('m520_a.txt', 'm520_d.txt');
  WriteLn('Move: ', TFile.Exists('m520_a.txt'), ' ', TFile.ReadAllText('m520_d.txt'));

  B := TEncoding.UTF8.GetBytes('one'#13#10'two'#10'three'#13'four'#10);
  TFile.WriteAllBytes('m520_lines.txt', B);
  ShowLines('ReadAllLines', TFile.ReadAllLines('m520_lines.txt'));

  WriteLn(TFile.Exists('m520_d.txt'), ' ', TDirectory.Exists('m520_d.txt'));
  TDirectory.CreateDirectory('m520_dir');
  TDirectory.CreateDirectory('m520_dir/sub');
  TFile.WriteAllText('m520_dir/b.txt', '1');
  TFile.WriteAllText('m520_dir/a.txt', '2');
  TFile.WriteAllText('m520_dir/c.dat', '3');
  TFile.WriteAllText('m520_dir/sub/d.txt', '4');
  WriteLn(TFile.Exists('m520_dir'), ' ', TDirectory.Exists('m520_dir'));
  ShowNames('GetFiles', TDirectory.GetFiles('m520_dir'));
  ShowNames('GetFiles *.txt', TDirectory.GetFiles('m520_dir', '*.txt'));
  ShowNames('GetFiles all', TDirectory.GetFiles('m520_dir', '*.txt',
    TSearchOption.soAllDirectories));
  ShowNames('GetDirectories', TDirectory.GetDirectories('m520_dir'));
  WriteLn('IsEmpty: ', TDirectory.IsEmpty('m520_dir'), ' ', TDirectory.IsEmpty('m520_dir/sub'));
  try
    TDirectory.Delete('m520_dir');
  except
    on E: Exception do Show('TDirectory.Delete not empty', E);
  end;
  WriteLn(TDirectory.Exists('m520_dir'));
  TDirectory.Delete('m520_dir', True);
  WriteLn(TDirectory.Exists('m520_dir'));

  WriteLn(TPath.GetExtension('a.tar.gz'), ' [', TPath.GetExtension('noext'), '] [',
    TPath.GetExtension('a.'), '] ', TPath.GetExtension('.bashrc'));
  WriteLn(TPath.GetFileNameWithoutExtension('a.tar.gz'), ' ',
    TPath.GetFileNameWithoutExtension('.bashrc'), ' ', TPath.GetFileName('plain'));
  WriteLn(TPath.ChangeExtension('a.txt', 'pas'), ' ', TPath.ChangeExtension('a.txt', '.pas'),
    ' ', TPath.ChangeExtension('b', 'x'), ' [', TPath.ChangeExtension('a.txt', ''), ']');
  WriteLn(TPath.HasExtension('a.txt'), ' ', TPath.HasExtension('a.'), ' ',
    TPath.HasExtension('a'));
  WriteLn('[', TPath.Combine('', 'b'), '] [', TPath.Combine('a', ''), ']');
  WriteLn(TPath.IsPathRooted('a'), ' [', TPath.GetDirectoryName('plain'), '] [',
    TPath.GetPathRoot('plain'), ']');
end.
