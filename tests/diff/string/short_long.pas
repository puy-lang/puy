program ShortLong;

// Strings up to a few dozen characters may be kept in another representation than longer ones
// (design/vm.md 3.3): the results are the same on both sides of the boundary, when a string grows
// across it in place, doubles itself, takes a non-ASCII character, and when a short value is
// compared with, searched in, or joined to a long one.

uses System.SysUtils;

var
  S, T, U: string;
  I: Integer;
  A: array of string;

begin
  // Growing one character at a time across the boundary, in place
  S := '';
  for I := 1 to 40 do
  begin
    S := S + Chr(Ord('a') + (I - 1) mod 26);
    if (I >= 28) and (I <= 33) then
      WriteLn(I, ' ', Length(S), ' ', S);
  end;

  // Appending a string to itself on both sides of the boundary
  S := 'abcdefghijklmno';
  S := S + S;
  WriteLn(Length(S), ' ', S);
  S := 'abcdefghijklmnop';
  S := S + S;
  WriteLn(Length(S), ' ', S);

  // A copy keeps its value when the original grows
  S := 'short';
  T := S;
  S := S + ' and then some more text past the boundary';
  WriteLn(T, '|', S);

  // Writing a character, ASCII and not, into a short string and a shared one
  S := 'hello';
  T := S;
  S[1] := 'J';
  WriteLn(S, ' ', T);
  S[5] := 'ö';
  WriteLn(S, ' ', Length(S), ' ', T);

  // A short string joined to a non-ASCII one, and appended with one
  S := 'abc';
  S := S + 'é';
  WriteLn(S, ' ', Length(S));
  S := 'x';
  T := S + 'ü' + IntToStr(12345);
  WriteLn(T, ' ', Length(T));

  // Comparing and searching between short and long values with the same start
  S := 'abcdefghij';
  T := 'abcdefghijklmnopqrstuvwxyz0123456789';
  WriteLn(S = T, ' ', S < T, ' ', T > S, ' ', S = Copy(T, 1, 10), ' ', Pos(S, T), ' ', Pos('789', T));
  U := Copy(T, 1, 30) + Copy(T, 31, 6);
  WriteLn(U = T, ' ', U, ' ', Length(U));

  // Case changes and slices on both sides of the boundary
  WriteLn(UpperCase(S), ' ', UpperCase(T), ' ', LowerCase('ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789'));
  WriteLn(Copy(T, 5, 30), ' ', Copy(T, 5, 31), ' ', Copy(T, 5, 32));

  // Numbers as text, and many short values held at once
  SetLength(A, 5);
  for I := 0 to 4 do
    A[I] := IntToStr(I * 1000003) + '-' + IntToStr(-I);
  for I := 0 to 4 do
    Write(A[I], ' ');
  WriteLn;
  WriteLn(IntToStr(High(Int64)), ' ', IntToStr(Low(Int64)));
end.
