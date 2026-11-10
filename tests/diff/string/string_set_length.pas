program StringSetLength;

// `SetLength` of a string (types.md chapter 7): shrinking keeps the start, growing keeps the
// old characters, and another reference to the string keeps its value.

var
  S, T: string;
  I: Integer;
begin
  S := 'hello';
  T := S;
  SetLength(S, 3);
  WriteLn(S, ' ', T, ' ', Length(S));
  SetLength(S, 6);
  WriteLn(Length(S), ' ', Copy(S, 1, 3));
  for I := 4 to 6 do
    S[I] := Chr(Ord('0') + I);
  WriteLn(S);
  SetLength(T, 0);
  WriteLn('[', T, '] ', Length(T));
  S := 'é日本';
  SetLength(S, 2);
  WriteLn(S);
end.
