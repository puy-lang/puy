program CharCodeAstral;

// A `#` character code is 0..$10FFFF; codes of 65536 and above expand to a surrogate pair (lexical.md 3.5; same as FPC)

var
  S: string;
begin
  S := #$1F600;
  WriteLn(S, ' ', Length(S), ' ', Ord(S[1]), ' ', Ord(S[2]));
  WriteLn(#65536#$10FFFF'x'#$D83D#$DE00 = #$10000 + #1114111 + 'x' + #$1F600);
  WriteLn(Length(#$10FFFF), Length(#$FFFF), Length('a'#$1F600'b'));
end.
