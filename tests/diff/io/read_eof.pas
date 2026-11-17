program ReadEof;

// Standard input (semantics.md 6.2): `Read` of several integers on one line and across lines,
// `ReadLn` of numbers then the rest of the line skipped, `ReadLn` of a string, `Read` of a
// `Char`, and `Eof` after the last line has been read with `ReadLn`.

var
  A, B, C, Sum, Lines: Integer;
  Ch: Char;
  S: string;
begin
  Read(A, B);
  Read(C);
  ReadLn; // `ReadLn` does not cross lines for numbers in Puy (delphi-diff.md), so end the line
  WriteLn(A + B + C);
  ReadLn(A, B);
  WriteLn(A * B);
  Read(Ch);
  Read(Ch);
  ReadLn(S);
  WriteLn('[', Ch, '] [', S, ']');
  ReadLn;
  Sum := 0;
  Lines := 0;
  while not Eof do
  begin
    ReadLn(A);
    Sum := Sum + A;
    Inc(Lines);
  end;
  WriteLn(Lines, ' lines, sum ', Sum);
end.
