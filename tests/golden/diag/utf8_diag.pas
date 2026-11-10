program Utf8Diag;

// Diagnostics of `UTF8String` (types.md 8.5): its bytes are read only (the E0707 variant with
// a note), `for..in` over it (E0730), a `var` parameter takes only the same type (E0702), a
// byte is a `Byte`, not a `Char` (E0702), conversions it does not have (E0724), operators it
// does not have (E0703), a byte passed to `Inc` (E0706), and `High` of it, which waits for the
// string `Low` / `High` (E3200).

procedure Change(var S: string);
begin
  S := S + '!';
end;

var
  U: UTF8String;
  C: Char;
  B: Byte;
begin
  U := 'abc';
  U[1] := 65;
  for C in U do
    WriteLn(C);
  Change(U);
  C := U[1];
  B := U[1];
  B := Integer(U);
  U := UTF8String(5);
  WriteLn(U + 1);
  WriteLn(U * U);
  Inc(U[1]);
  WriteLn(High(U));
  WriteLn(B);
end.
