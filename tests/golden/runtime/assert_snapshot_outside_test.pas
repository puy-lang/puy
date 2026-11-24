// AssertSnapshot outside `puy test` has no runner to compare with: ENotSupportedException, which
// a program can catch (docs/spec/stdlib/Puy.Testing/README.md 5).
program AssertSnapshotOutsideTest;

uses
  System.SysUtils, Puy.Testing;

begin
  try
    AssertSnapshot('value');
  except
    on E: ENotSupportedException do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  AssertSnapshot('value', 'named');
end.
