// puy-test: test-fs
program CurrentDirRemoved;

// A failure of the host that GetCurrentDir cannot turn into False (spec/stdlib/System.SysUtils/
// Files.md): the working directory was removed, so the runner's OS reports `NotFound` with the
// OS code 2, and the exception is an EInOutError with that kind and code.

uses
  System.SysUtils;

begin
  WriteLn(CreateDir('gone'), SetCurrentDir('gone'), RemoveDir('/work/gone'));
  try
    WriteLn(GetCurrentDir);
  except
    on E: EInOutError do
      WriteLn(E.ClassName, ': ', E.Message, ' (', E.ErrorKind, ', ', E.ErrorCode, ')');
  end;
  WriteLn(SetCurrentDir('/work'), ' ', GetCurrentDir);
end.
