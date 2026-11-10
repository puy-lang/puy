program SliceUtf8;

// A slice of a `UTF8String` counts bytes and is a `UTF8String`; cutting inside a multibyte
// character raises `EEncodingError` (caught, then uncaught at the start of a slice), and an empty
// slice is never checked (types.md 8.5, 8.7).

var
  U, V: UTF8String;
begin
  U := 'aあb';
  V := U[2..4];
  WriteLn(V, ' ', Length(V));
  WriteLn(U[..1], U[5..]);
  WriteLn('[', U[3..2], ']');
  try
    WriteLn(U[1..2]);
  except
    on E: EEncodingError do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
  WriteLn(U[3..]);
end.
