// `raise X` with a nil `X` raises `EAccessViolation` instead (docs/spec/exceptions.md 1, 5.1).
var E: Exception := nil;
WriteLn('before');
raise E;
