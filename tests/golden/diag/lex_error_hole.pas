program LexErrorHole;
var
  S: string;
  N, G: Integer;
begin
  G := 1;
  S := 'abc;
  WriteLn(S);
  S := 'abc
    + S;
  N := Chr(0x3042);
  N := $G + 1;
  WriteLn(N, 0x12);
  // Lexical error at the start of a statement (grammar.md chapter 3): the hole becomes the statement's leading expression, with no chained E0402 / E0707
  あ := 1;
  あ(1, N);
  あ
  WriteLn(N);
end.
