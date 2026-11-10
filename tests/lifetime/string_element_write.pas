program StringElementWrite;

// Writing a string element (`S[I] := C`): a string with one reference is written in place,
// a shared one is copied first, so the literal and the other variable keep their value
// (bytecode.md 2.4; semantics.md 3.5).

procedure Run;
var
  S, T: string;
begin
  S := 'abc';
  S[1] := 'x';
  T := S;
  S[2] := 'y';
  S[3] := 'é';
  WriteLn(S, ' ', T, ' ', 'abc');
end;

begin
  Run;
  Run;
end.
