// puy-test: no-fs
program PathsNoFs;

// TFilePath without `Fs` (spec/stdlib/Puy.Paths/README.md): taking paths apart still works;
// Resolve, Exists and the file operations raise ENotSupportedException as System.IOUtils does.

uses
  System.SysUtils, Puy.Paths;

procedure Attempt(const What: string; Op: Integer);
begin
  try
    case Op of
      0: TFilePath('a').Resolve;
      1: TFilePath('a').Exists;
      2: TFilePath('a').ReadAllText;
      3: TFilePath('.').Entries;
    end;
    WriteLn(What, ': no exception');
  except
    on E: ENotSupportedException do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message, ' (', E.ErrorKind, ')');
  end;
end;

begin
  WriteLn((TFilePath('a') / 'b.txt').Normalize.Stem);
  Attempt('Resolve', 0);
  Attempt('Exists', 1);
  Attempt('ReadAllText', 2);
  Attempt('Entries', 3);
end.
