// puy-test: stack-limit=64k
program OverflowWithArgs;

// A call that overflows the stack never enters the callee, so its string value arguments, made
// for the callee to own, are released by the failed call itself (lifetime.md 3.3, 3.8).

procedure Deep(S: string; N: Integer);
begin
  Deep(S + 'x', N + 1);
end;

begin
  Deep('a', 0);
end.
