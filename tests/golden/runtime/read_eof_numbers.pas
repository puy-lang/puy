program ReadEofNumbers;

// `while not Eof do Read(N)` sums numbers spread over lines: at the end of the last line `Eof`
// is already `True`, even with trailing blanks after the last number, so the loop does not read
// past the end (semantics.md 6.2).

var
  N, Sum, Count: Integer;
begin
  Sum := 0;
  Count := 0;
  while not Eof do
  begin
    Read(N);
    Inc(Sum, N);
    Inc(Count);
  end;
  WriteLn(Count, ' ', Sum);
end.
