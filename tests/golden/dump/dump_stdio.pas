program DumpStdio;

// The hir and bytecode dumps of standard I/O (semantics.md 9, bytecode.md 2.17): `Read` /
// `ReadLn` between `ReadLnBegin` and `ReadLnEnd` (the last number of `ReadLn` takes the rest of
// the line), `Write(ErrOutput, …)` through `Console.WriteError`, `Flush`, `Eof` / `Eoln`.

var
  I: Integer;
  C: Char;
  S: string;
  D: Double;
  M: Currency;
begin
  ReadLn(I, C, S);
  Read(Input, D, I, M);
  ReadLn;
  WriteLn(ErrOutput, 'e', sLineBreak, I);
  Write(Output, C, D, M);
  Flush(Output);
  Flush(ErrOutput);
  if Eof or Eoln(Input) then
    WriteLn(S);
end.
