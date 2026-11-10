// `resourcestring` at the top level of a script (semantics.md 1.2, 3.6): the names are global
// constants seen from routines, a one-character value is a `string`, and a section may follow
// statements.

resourcestring
  SName = 'Puy';
  SDot = '.';

procedure Greet;
begin
  WriteLn('Hello from ', SName, SDot);
end;

procedure Show(C: Char); overload;
begin
  WriteLn('Char: ', C);
end;

procedure Show(const S: string); overload;
begin
  WriteLn('string: ', S);
end;

Greet;
Show(SDot);
WriteLn(Length(SDot));

resourcestring
  SDone = 'done';

WriteLn(SDone)
