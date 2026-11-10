program ReadMulti;

// The input line buffer (semantics.md 6.2): several variables in one `ReadLn`, `Read` going on
// across lines, trailing blanks after the last number, `Char` (the line break `#10` at the end
// of a line), reals and `Currency`, `Eoln` / `Eof` on the last line.

var
  A, B: Integer;
  C, D: Char;
  S: string;
  R: Double;
  F: Single;
  M: Currency;
begin
  ReadLn(A, B);
  WriteLn(A + B);
  Read(A);
  Read(B);
  WriteLn(A, ' ', B);
  ReadLn(Input, A);
  WriteLn(A);
  ReadLn(C);
  WriteLn(Ord(C));
  Read(C, D);
  WriteLn(C, D);
  Read(C);
  WriteLn(Ord(C));
  ReadLn(R, F, M);
  WriteLn(R, ' ', F, ' ', M);
  ReadLn(S, C);
  WriteLn('[', S, '] ', Ord(C));
  while not Eoln(Input) do
  begin
    Read(C);
    Write(C);
  end;
  WriteLn('|', Eoln, ' ', Eof);
  ReadLn;
  WriteLn(Eof(Input));
  ReadLn(S);
  WriteLn('[', S, ']');
end.
