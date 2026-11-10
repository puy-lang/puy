program ResourceStrings;

// `resourcestring` in the declaration part of a program (semantics.md 3.6): the values are
// string constants, a one-character value is a `string` (not a `Char`), and the value may be
// a constant expression.

const
  Greeting = 'Hello';

resourcestring
  SHello = Greeting + ', world';
  SOne = 'x';
  SEmpty = '';

procedure Show(C: Char); overload;
begin
  WriteLn('Char: ', C);
end;

procedure Show(const S: string); overload;
begin
  WriteLn('string: ', S);
end;

resourcestring
  SLater = 'declared after a routine';

var
  S: string;
begin
  WriteLn(SHello);
  WriteLn(Length(SOne));
  Show(SOne);
  Show('y');
  WriteLn(Length(SEmpty));
  S := SOne + SOne;
  WriteLn(S);
  WriteLn(SLater);
  if SOne = 'x' then
    WriteLn('equal');
end.
