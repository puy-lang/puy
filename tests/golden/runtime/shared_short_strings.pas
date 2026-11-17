program SharedShortStrings;

// The one-character ASCII strings and the decimal text of 0..99 come from a table of shared
// strings (design/vm.md 3.11), so several variables refer to one string. Writing a character,
// appending, SetLength and a var parameter must still change only the variable written, and the
// values just outside the table (#128, -1, 100) are made as before. The leak check at the end
// counts no block (the table lets go of its strings before it).

uses System.SysUtils;

procedure Bump(var S: string);
begin
  S[1] := 'q';
end;

var
  A, B, C, D: string;
  X: string;
  I, N: Integer;
begin
  X := 'abc';
  A := X[1];
  B := X[1];
  A[1] := 'z';
  WriteLn(A, ' ', B, ' ', X[1]);

  A := IntToStr(7);
  A := A + 'x';
  B := IntToStr(7);
  WriteLn(A, ' ', B);

  // The code goes through a variable: Char(65) would be folded into a constant
  N := 65;
  C := Char(N);
  SetLength(C, 3);
  C[2] := 'B';
  C[3] := 'C';
  D := Char(N);
  WriteLn(C, ' ', D, ' ', Char(N));

  D := IntToStr(42);
  Bump(D);
  WriteLn(D, ' ', IntToStr(42));

  for I := -1 to 0 do
    Write(IntToStr(I), ' ');
  WriteLn(IntToStr(99), ' ', IntToStr(100));
  N := 127;
  A := Char(N);
  N := 128;
  B := Char(N);
  WriteLn(Length(A), ' ', Ord(A[1]), ' ', Length(B), ' ', Ord(B[1]));
end.
