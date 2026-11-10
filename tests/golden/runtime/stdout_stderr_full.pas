// puy-test: merge-stderr, stdout-buffering=full
program StdoutStderrFull;

// Standard output is written out before each write to standard error, so the two streams keep
// the program's order whatever the buffering (semantics.md 6.1). The capture merges them like
// `2>&1`.

begin
  Write('a');
  WriteLn(ErrOutput, 'E1');
  WriteLn('b');
  Write(ErrOutput, 'E2 ', 1);
  WriteLn(ErrOutput);
  WriteLn(Output, 'c');
  Flush(Output);
  Flush(ErrOutput);
  Write(ErrOutput);
  Write('d' + sLineBreak + 'e');
  WriteLn(ErrOutput, 'E3');
  WriteLn;
end.
