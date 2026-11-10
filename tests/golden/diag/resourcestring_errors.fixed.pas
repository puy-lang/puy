program ResourceStringErrors;

// Diagnostics of `resourcestring` (semantics.md 3.6): a section inside a routine (E0416, right
// after the heading and after a local section; the names stay usable), a written type (E0417;
// the fix is safe only for `string`), a value that is not a string (E0702), and a value that
// is not a constant expression (E0713). A conditional directive between the declarations is at a
// boundary (no W0005; directives.md 3.1).

procedure First;
resourcestring
  SFirst = 'first';
begin
  WriteLn(SFirst);
end;

procedure Second;
var
  I: Integer;
resourcestring
  SSecond = 'second';
begin
  I := Length(SSecond);
  WriteLn(I);
end;

var
  G: string = 'g';

resourcestring
  STyped = 'typed';
  SWrongType : Integer = 'wrong';
  SNumber = 42;
  SVariable = G;
  {$IFDEF NEVER}
  SChar = #66;
  {$ELSE}
  SChar = #65;
  {$ENDIF}

begin
  First;
  Second;
  WriteLn(STyped, SWrongType, SChar);
end.
