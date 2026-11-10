// The tree of `resourcestring` (grammar.md 1.3): a section at the top level, and a section that
// starts the body of a routine (E0416; the body is still read).

resourcestring
  A = 'a';
  B: string = 'b';

procedure P;
resourcestring
  C = 'c';
begin
end;
