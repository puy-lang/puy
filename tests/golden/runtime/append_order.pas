program AppendOrder;

// T := T + X1 + ... appends to the place T in place when nothing else refers to its string
// (types.md 9.1), but the value is still the one of the left-to-right order (semantics.md 3.1):
// T is read before the operands run, so an operand that changes T (or replaces the array that
// holds it) does not change what is appended to, and the result goes to the place as it is after
// the operands. A for..in over the string iterates the string it started with. A UTF8String
// with string operands gets the same bytes as converting the whole string sum.

uses System.SysUtils;

type
  TRec = record
    S: string;
  end;

var
  G: string;
  R: TRec;
  A: array of string;
  U, V: UTF8String;
  C: Char;

function Grow: string;
begin
  G := G + '+';
  Result := '[' + G + ']';
end;

function Replace: string;
begin
  G := 'new';
  Result := '!';
end;

function Field: string;
begin
  R.S := R.S + '*';
  Result := '#';
end;

function Resize: string;
begin
  A := nil;
  SetLength(A, 1);
  A[0] := 'z';
  Result := '%';
end;

begin
  G := 'g';
  G := G + Grow + 'x';
  WriteLn(G);
  G := G + Replace;
  WriteLn(G);
  R.S := 'r';
  R.S := R.S + Field;
  WriteLn(R.S);
  SetLength(A, 2);
  A[0] := 'a';
  A[0] := A[0] + Resize;
  WriteLn(A[0], ' ', Length(A));
  G := 'ab';
  for C in G do
    G := G + C;
  WriteLn(G);
  U := 'é';
  U := U + 'x' + #$D83D;
  U := U + #$DE00;
  V := 'ü';
  U := U + V;
  U := U + U;
  WriteLn(U, ' ', Length(U));
end.
