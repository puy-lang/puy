program DumpUtf8;

// The `hir` and `bytecode` dumps of `UTF8String` (types.md 8.5): `utf8` constants (kind 7),
// the `string->utf8` / `utf8->string` conversions, `utf8` concatenation and comparison, and the
// byte built-ins (`U8Len`, `U8Index`, `U8Copy`, `U8Pos`, `U8SetLen`).

var
  U, V: UTF8String;
  S: string;
  N: Integer;
begin
  U := 'é';
  S := 'x';
  V := S;
  V := U + V;
  S := U + S;
  N := Length(V) + V[1] + Pos(U, V);
  if U < V then
    V := Copy(V, 1, 2);
  SetLength(V, N);
  WriteLn(V, S = U);
end.
