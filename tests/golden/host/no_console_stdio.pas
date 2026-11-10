// puy-test: no-console
program NoConsoleStdio;

// Without a `Console` (semantics.md 6.1, 6.2), writes to standard output and standard error and
// `Flush` are discarded, and reading raises `ENotSupportedException`.

begin
  Write('x');
  WriteLn(ErrOutput, 'nobody hears this');
  Flush(Output);
  WriteLn(Eof);
end.
