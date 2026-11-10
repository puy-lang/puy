// At script top level too, a procedure right after the header is a nested routine (semantics.md 4.9).
// Without a begin after the declaration part, the following statements are read as top-level statements, not guessed as the body
// (grammar.md 1.3; no fix inserting begin)
procedure P;
procedure Q;
begin
end;
WriteLn('top');
