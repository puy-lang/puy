program StdinInvalidUtf8;

// Invalid UTF-8 sequences on standard input become U+FFFD (semantics.md 6.2; done by the native runtime)

var
  S: string;
begin
  ReadLn(S);
  WriteLn(S, ' ', Length(S));
end.
