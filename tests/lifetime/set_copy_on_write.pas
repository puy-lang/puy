program SetCopyOnWrite;

// A big set (types.md 8.3: a base type with ordinals above 63) is a copy-on-write heap block,
// like a record. After `B := A`, `Include` / `Exclude` and the operators make a copy before
// changing one of them, so the other keeps its elements. The constant in the constant cache
// is never changed either, and every block is freed (no leak).

type
  TChars = set of Char;

const
  Vowels = ['a', 'e', 'i', 'o', 'u'];

procedure Show(const S: string; const C: TChars);
var
  Ch: Char;
begin
  Write(S, ':');
  for Ch in C do
    Write(' ', Ch);
  WriteLn;
end;

procedure Run;
var
  A, B: TChars;
begin
  A := Vowels;
  B := A;
  Include(B, 'y');
  Exclude(A, 'a');
  Show('A', A);
  Show('B', B);
  A := B;
  B := B - ['e'..'o'];
  Show('A', A);
  Show('B', B);
  Show('Vowels', Vowels);
end;

begin
  Run;
  Run;
end.
