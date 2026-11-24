// puy-test: no-fs, absent=unsupported
program FsUnsupported;

// A target without a file system (the browser; spec/stdlib/System.SysUtils/Files.md): the file
// functions raise ENotSupportedException with ErrorKind ekUnsupported, which a program tells from
// a capability the embedding did not grant (ekNoPermission).

uses
  System.SysUtils;

begin
  try
    WriteLn(FileExists('a.txt'));
  except
    on E: ENotSupportedException do
    begin
      WriteLn(E.Message);
      WriteLn(E.ErrorKind = TIOErrorKind.ekUnsupported, ' ', E.ErrorKind = TIOErrorKind.ekNoPermission);
    end;
  end;
end.
