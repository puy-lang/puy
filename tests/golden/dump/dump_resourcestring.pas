program DumpResourceString;

// The `hir` dump of `resourcestring` (semantics.md 3.6, 9): a use is a `string` constant, also
// for a one-character value, and a value built from a constant expression is folded.

const
  Prefix = 'Error: ';

resourcestring
  SFailed = Prefix + 'failed';
  SColon = ':';

var
  S: string;
begin
  S := SColon;
  WriteLn(SFailed, S, Length(SColon));
end.
