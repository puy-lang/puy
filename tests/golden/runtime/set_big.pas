program SetBig;

// Big sets (types.md 8.3): `set of Char` over every UTF-16 code unit and `set of Word`, which
// FPC does not have (its sets hold at most 256 values), so the case is golden. Union,
// difference, intersection, comparisons, `in`, `Include` / `Exclude`, `for..in`, the
// constant without a type `const Digits = ['0'..'9'];` (a `set of Char`), a set of a subrange
// of `Char` assigned from and to `set of Char`, and `in` with a left operand outside 0..65535
// (`False`, not an exception).

type
  TChars = set of Char;
  TWords = set of Word;
  TLower = set of 'a'..'z';

const
  Digits = ['0'..'9'];
  Hex: TChars = ['0'..'9', 'A'..'F'];

procedure ShowChars(const S: TChars);
var
  C: Char;
begin
  Write('[');
  for C in S do
    Write(C);
  WriteLn(']');
end;

procedure ShowWords(const S: TWords);
var
  W: Word;
begin
  Write('[');
  for W in S do
    Write(' ', W);
  WriteLn(' ]');
end;

var
  A, B: TChars;
  W: TWords;
  L: TLower;
  I: Integer;
  N: Int64;
  C: Char;
begin
  A := ['a'..'f', 'あ', #$FFFF];
  B := Hex + ['x', 'a'..'c'];
  ShowChars(A * B);
  ShowChars(B - Digits);
  WriteLn('あ' in A, ' ', 'い' in A, ' ', #$FFFF in A, ' ', 'x' in B);
  WriteLn(Digits <= Hex, ' ', Hex <= Digits, ' ', A = A + [], ' ', A <> B, ' ', B >= ['A', 'x']);
  C := '7';
  WriteLn(C in Digits, ' ', C in ['a'..'z'], ' ', 'Q' in ['A'..'Z']);
  Include(A, 'z');
  Exclude(A, 'あ');
  Exclude(A, #$FFFF);
  ShowChars(A);

  W := [0, 1000..1002, 65535];
  ShowWords(W);
  ShowWords(W * [1001..65534]);
  Include(W, 40000);
  Exclude(W, 0);
  ShowWords(W);
  WriteLn(65535 in W, ' ', 2 in W);
  I := 65536;
  WriteLn(I in W, ' ', I in [1, 2], ' ', -1 in W);
  N := 4294967296 + 1000;
  WriteLn(N in W);

  L := ['q', 'b'];
  A := L;
  Include(A, '0');
  ShowChars(A);
  A := ['c', 'k'];
  L := A;
  for C in L do
    Write(C);
  WriteLn;
end.
