// At script top level too, a type right after the header is a local section (like const / var).
// The body is read from the begin after the declaration part (grammar.md 1.3)
procedure P;
type
  TCount = Integer;
begin
  WriteLn('p');
end;
P;
