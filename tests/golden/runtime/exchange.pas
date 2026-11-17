program ExchangeElements;

// `Exchange(A, I, J)` swaps two elements of a dynamic or static array (types.md chapter 7):
// `I = J` does nothing, both indices are range-checked, and a static array shared with another
// variable is made unique first.

type
  TArr = array[1..3] of string;

var
  D: array of string;
  S, T: TArr;
  I: Integer;

begin
  D := ['a', 'b', 'c'];
  Exchange(D, 0, 2);
  I := 1;
  Exchange(D, I, I);
  WriteLn(D[0], D[1], D[2]);
  S[1] := 'x';
  S[2] := 'y';
  S[3] := 'z';
  T := S;
  Exchange(S, 1, 3);
  WriteLn(S[1], S[2], S[3], ' ', T[1], T[2], T[3]);
  try
    Exchange(D, 1, 3);
  except
    on E: ERangeError do
      WriteLn(E.Message);
  end;
  I := 4;
  try
    Exchange(S, 1, I);
  except
    on E: ERangeError do
      WriteLn(E.Message);
  end;
  WriteLn(D[0], D[1], D[2], ' ', S[1], S[2], S[3]);
end.
