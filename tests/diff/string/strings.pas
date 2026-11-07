program Strings;

var
  S, T: string;
  C: Char;
  I: Integer;
begin
  S := 'Hello';
  T := S + ', ' + 'World';
  WriteLn(T);
  WriteLn(Length(T));
  WriteLn(Copy(T, 8, 5));
  WriteLn(Pos('World', T));
  WriteLn(Pos('xyz', T));
  C := T[1];
  WriteLn(C);
  T[1] := 'J';
  WriteLn(T);
  WriteLn(S);
  if S = 'Hello' then
    WriteLn('same');
  if S <> T then
    WriteLn('different');
  S := '';
  WriteLn('[', S, ']', Length(S));
  for I := 1 to 3 do
    S := S + Chr(Ord('a') + I);
  WriteLn(S);
  WriteLn(Ord('A'));
  WriteLn(Chr(66));
  WriteLn('tab', #9, 'end');
  WriteLn('it''s');
  WriteLn('日本語', Length('日本語'));
  T := 'abc';
  WriteLn(T + T);
  WriteLn(Copy(T, 2, 100));
  WriteLn(Copy(T, 5, 1), '|');
  WriteLn('x':3, '|');
  WriteLn('wide':2, '|');
end.
