// puy-test: test-ext
// An exception a callback does not handle keeps the class and the stack trace it was raised
// with when the extension returns it (`docs/spec/extensions.md` 5.2, 5.3): the trace goes
// through the call of the extension.
program ExtCallbackTrace;

uses TestExt;

procedure Run;
begin
  Apply(function(X: Integer): Integer
    begin
      raise ETestError.Create('deep');
    end, 1);
end;

begin
  Run;
end.
