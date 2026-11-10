program ReadEofLines;

// `while not Eof do ReadLn(S)` reads every line, the empty ones and a last line of only blanks
// without a line break included (semantics.md 6.2).

var
  S: string;
  N: Integer;
begin
  N := 0;
  while not Eof do
  begin
    ReadLn(S);
    Inc(N);
    WriteLn(N, ': [', S, ']');
  end;
  WriteLn(Eof, ' ', Eoln);
end.
