program DumpEmptyFinally;

// The `bytecode` dump of empty `finally` blocks (exceptions.md 6.3): the handler range of an
// empty block ends where it starts, at its `EndFinally`; a call of an empty routine in a
// `finally` block is inlined like any other call, leaving the block empty.

procedure Nothing;
begin
end;

var
  N: Integer;
begin
  try
    N := 2;
  finally
  end;
  try
    N := N + 1;
  finally
    Nothing;
  end;
  WriteLn(N);
end.
