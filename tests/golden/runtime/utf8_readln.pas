program Utf8ReadLn;

// `ReadLn` into a `UTF8String` (semantics.md 6.2, types.md 8.5): the line becomes its UTF-8
// bytes; at the end of the input it is empty.

var
  U: UTF8String;
begin
  ReadLn(U);
  WriteLn('[', U, '] ', Length(U));
  ReadLn(U);
  WriteLn('[', U, '] ', Length(U));
  ReadLn(U);
  WriteLn('[', U, '] ', Length(U));
end.
