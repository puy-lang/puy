program StringPaths;

// Pos, UpperCase / LowerCase and chained concatenation across the one-byte and UTF-16 forms of
// a string: an ASCII needle in a non-ASCII string and the other way round, a UTF-16 string left
// with ASCII characters only, and chains whose intermediate value is extended in place while the
// operands stay unchanged.

uses
  System.SysUtils;

var
  A, B, C, S, U: string;
  I: Integer;
  N: Int64;
begin
  S := 'aébécé';
  WriteLn(Pos('b', S), ' ', Pos('cé', S), ' ', Pos('é', S), ' ', Pos('ée', S));
  WriteLn(Pos('é', 'abcde'), ' ', Pos('abc', 'ab'), ' ', Pos('de', 'abcde'), ' ', Pos('aab', 'aaab'));
  // A UTF-16 string whose only non-ASCII character is replaced
  U := 'é' + 'abc';
  U[1] := 'x';
  WriteLn(U, ' ', Pos('xa', U), ' ', Pos(U, 'zxabc'), ' ', Pos('c', U), ' ', Pos('xb', U));
  WriteLn(UpperCase(U), ' ', LowerCase('X' + U), ' ', UpperCase(S), ' ', LowerCase('ÉA'));
  // Chained concatenation: the operands keep their values
  A := 'ab';
  B := A + IntToStr(1) + '-' + A;
  WriteLn(A, ' ', B);
  C := B + 'é' + A + IntToStr(-23);
  WriteLn(A, ' ', B, ' ', C, ' ', Length(C));
  C := IntToStr(5) + IntToStr(6) + IntToStr(7) + C;
  WriteLn(C);
  A := '';
  for I := 1 to 3 do
    A := IntToStr(I) + ':' + A + '/' + IntToStr(I * I);
  WriteLn(A);
  N := High(Int64);
  WriteLn(IntToStr(N) + ' ' + IntToStr(-N) + ' ' + IntToStr(Low(Int64)) + ' ' + IntToStr(0));
end.
