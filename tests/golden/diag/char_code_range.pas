program CharCodeRange;

// A `#` character code above $10FFFF is E0008 (lexical.md 3.5)

begin
  WriteLn(#$110000);
  WriteLn(#1114112);
  WriteLn(#$1F600);
end.
