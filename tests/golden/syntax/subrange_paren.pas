program SubrangeParen;

// A type that starts with `(` is a subrange when the matching `)` is followed by `..`, and
// an enumeration otherwise (types.md 8.2). A bound ends before `=`, so a typed constant of
// a subrange type reads its value after it.

const
  A = 1;

type
  TFromParen = (A + 1)..5;
  TEnum = (B, C);
  TFromName = A..A + 2;

const
  X: (A + 1)..5 = 4;

var
  P: TFromParen;
  N: TFromName;

begin
  P := 2;
  N := 3;
  WriteLn(Low(TFromParen), ' ', High(TFromParen), ' ', P);
  WriteLn(Low(TFromName), ' ', High(TFromName), ' ', N);
  WriteLn(Ord(High(TEnum)), ' ', X);
end.
