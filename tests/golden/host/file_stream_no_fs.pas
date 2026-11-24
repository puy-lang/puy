// puy-test: no-fs
program FileStreamNoFs;

// Without `Fs` (an embedding that did not grant it; spec/stdlib/System.Classes/TStream.md),
// TFileStream.Create raises ENotSupportedException with ErrorKind ekNoPermission, not
// EFOpenError; streams in memory work, and SaveToFile raises the same.

uses
  System.SysUtils, System.Classes;

begin
  try
    TFileStream.Create('a.bin', fmOpenRead);
  except
    on E: ENotSupportedException do
      WriteLn(E.ClassName, ': ', E.Message, ' ', E.ErrorKind = TIOErrorKind.ekNoPermission);
  end;
  var M := TStringStream.Create('abc');
  WriteLn(M.DataString);
  try
    M.SaveToFile('m.bin');
  except
    on E: ENotSupportedException do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end.
