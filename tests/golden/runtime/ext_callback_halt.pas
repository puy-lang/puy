// puy-test: test-ext
// `Halt` in a callback ends the program from the extension's call (`docs/spec/extensions.md`
// 5.2): the `finally` blocks around the call run, and the extension cannot stop it.
program ExtCallbackHalt;

uses TestExt;

begin
  try
    WriteLn(TryApply(function(X: Integer): Integer
      begin
        WriteLn('halting');
        Halt(3);
      end, 1));
    WriteLn('not reached');
  finally
    WriteLn('finally');
  end;
end.
