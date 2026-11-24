// puy-test: test-ext
// A suspendable function of an extension waits like a host call (`docs/spec/extensions.md`
// 5.1): its future is not done when first polled, the task waits, and the result arrives when
// the task runs again.
program ExtAsync;

uses TestExt;

var
  I, Sum: Integer;
begin
  Sum := 0;
  for I := 1 to 5 do
    Sum := Sum + SlowAdd(I, 100);
  WriteLn(Sum);
  WriteLn(SlowAdd(1, 2) + SlowAdd(3, 4));
end.
