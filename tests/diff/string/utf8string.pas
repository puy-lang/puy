program Utf8Str;

// `UTF8String` (types.md 8.5): `Length` counts bytes, `S[I]` reads a byte, `+` and the
// comparisons of two `UTF8String`s, mixing with `string`, and the round trip through `string`
// keeps every character. Bytes are written with `Ord` (FPC reads an element as `AnsiChar`).

var
  U, V: UTF8String;
  S: string;
  I: Integer;
begin
  U := 'aé日🙂';
  WriteLn(Length(U));
  for I := 1 to Length(U) do
    Write(Ord(U[I]), ' ');
  WriteLn;
  S := U;
  WriteLn(S, ' ', Length(S));
  V := S;
  WriteLn(V = U, ' ', Length(V));
  V := U + 'x';
  WriteLn(V, ' ', Length(V));
  WriteLn(U < V, ' ', V < U, ' ', U <> V, ' ', U >= U);
  V := 'é';
  U := 'z';
  WriteLn(U < V);
  S := 'pre-' + V;
  WriteLn(S, ' ', Length(S));
  U := '';
  WriteLn('[', U, '] ', Length(U), ' ', U = '');
  U := UTF8String('ü') + UTF8String('ß');
  WriteLn(U, ' ', Length(U), ' ', string(U) = 'üß');
end.
