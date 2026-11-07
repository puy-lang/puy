program IncStringElem;

// Like var arguments, string elements cannot be the target of Inc / Dec / ReadLn (semantics.md 5.1; M1)

var
  S: string;
begin
  S := 'ab';
  Inc(S[1]);
  Dec(S[2], 1);
  ReadLn(S[1]);
  WriteLn(S);
end.
