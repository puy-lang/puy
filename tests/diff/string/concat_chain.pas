program ConcatChain;

// A sum of three or more strings is made at once (ConcatStrN; bytecode.md 2.4): the result is the
// same as joining left to right, whatever the operands are (literals, variables, Char values,
// results of calls, empty strings, non-ASCII text), and the operands keep their values.

uses System.SysUtils;

var
  G: string;

function Twice(const S: string): string;
begin
  Result := S + S;
end;

function Wrap(const S, L, R: string): string;
begin
  Result := L + S + R;
end;

procedure Locals;
var
  A, B, C, E, S: string;
  Ch: Char;
  N: Integer;
begin
  A := 'ab';
  B := 'cd';
  C := 'ef';
  E := '';
  Ch := '!';
  N := 42;
  S := A + B + C;
  WriteLn(S, ' ', Length(S));
  S := A + '-' + IntToStr(N) + '-' + B + Ch;
  WriteLn(S, ' ', Length(S));
  S := E + A + E;
  WriteLn(S, ' ', Length(S));
  S := E + E + E;
  WriteLn('[', S, '] ', Length(S));
  S := A + E + E + B;
  WriteLn(S, ' ', Length(S));
  S := 'é' + A + 'ü' + B;
  WriteLn(S, ' ', Length(S));
  S := A + B + 'あい' + C + Ch;
  WriteLn(S, ' ', Length(S));
  S := Twice(A) + Twice(B) + Twice(Ch);
  WriteLn(S, ' ', Length(S));
  S := A + A + A + A;
  WriteLn(S, ' ', A);
  S := 'x' + S + S;
  WriteLn(S, ' ', Length(S));
  WriteLn(Wrap(A + B + C, '<', '>'), ' ', Wrap(E, '(' + E + ')', Ch + Ch + Ch));
end;

begin
  Locals;
  G := 'g';
  G := '[' + G + ']' + G;
  WriteLn(G);
  G := G + '-' + G + '-' + G;
  WriteLn(G, ' ', Length(G));
end.
