program SetRangeCheck;

// Under `{$R+}`, an element out of the range of a set's base type is an ERangeError
// (types.md 8.3): `Include`, a constructor with a variable element or range bound, an
// assignment from a set type with a wider base, and `+` with a right operand of such a type
// (converted to the left operand's type). The message names the set type. An empty range
// `[L..H]` with `L > H` is not checked, and `*` / `-` drop what the left type cannot hold.
// Under `{$R-}` an element the set cannot hold is dropped. The last error is not caught.

uses System.SysUtils;

{$R+}

type
  TDigit = 0..9;
  TDigits = set of TDigit;
  TLower = set of 'a'..'z';

var
  D: TDigits;
  B: set of Byte;
  L: TLower;
  C: set of Char;
  I, J: Integer;
  Ch: Char;

procedure Check(N: Integer);
begin
  try
    case N of
      1: begin I := 12; Include(D, I); end;
      2: begin I := 20; D := [1, I]; end;
      3: begin I := 5; J := 20; D := [I..J]; end;
      4: begin I := 20; J := 3; D := [I..J]; WriteLn('4: empty range'); end;
      5: begin B := [5, 200]; D := B; end;
      6: begin B := [7, 100]; D := [1] + B; end;
      7: begin B := [7, 100]; D := [1, 7, 8] * B; WriteLn('7: ', 7 in D, ' ', 8 in D); end;
      8: begin C := ['a', '0']; L := C; end;
      9: begin Ch := '!'; Include(L, Ch); end;
    end;
  except
    on E: ERangeError do WriteLn(N, ': ', E.Message);
  end;
end;

begin
  for I := 1 to 9 do
    Check(I);
  {$R-}
  D := [];
  I := 70;
  Include(D, I);
  B := [9, 200];
  D := B;
  WriteLn(70 in D, ' ', 9 in D, ' ', D = [9]);
  {$R+}
  Ch := 'Z';
  L := ['a', Ch];
end.
