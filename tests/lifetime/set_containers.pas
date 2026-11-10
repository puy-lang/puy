program SetContainers;

// Big sets (types.md 8.3) held in a record field, a static array element, a dynamic array
// element, a typed record constant, a function result and a `var` parameter: `Include` and
// the operators change only the place written, and every block is freed (no leak).

type
  TChars = set of Char;
  TColor = (Red, Green, Blue);
  TColors = set of TColor;
  TRec = record
    Name: string;
    Cs: TChars;
    Ks: TColors;
  end;
  TArr = array[0..1] of TChars;
const
  R0: TRec = (Name: 'r'; Cs: ['a'..'c']; Ks: [TColor.Green]);
function Letters(const S: string): TChars;
var
  C: Char;
begin
  Result := [];
  for C in S do
    Include(Result, C);
end;
procedure AddX(var S: TChars);
begin
  Include(S, 'x');
  S := S + ['y'];
end;
procedure Show(S: TChars);
var C: Char;
begin
  for C in S do Write(C);
  WriteLn;
end;
var
  R: TRec;
  A: TArr;
  L: TArray<TChars>;
  K: TColor;
begin
  R := R0;
  Include(R.Cs, 'z');
  Include(R.Ks, TColor.Red);
  Show(R.Cs); Show(R0.Cs);
  for K in R.Ks do Write(K, ' ');
  WriteLn;
  A[1] := Letters('hello');
  AddX(A[1]);
  Show(A[1]);
  AddX(R.Cs);
  Show(R.Cs);
  SetLength(L, 2);
  L[0] := ['q'];
  Include(L[1], 'w');
  L[1] := L[1] + L[0];
  Show(L[1]);
  WriteLn('e' in Letters('hello'), ' ', 'h' in A[1]);
end.
