program DumpHashCombine;

// The ir and bytecode dumps of HashCombine: one HashMix of its two arguments (equality.md 4, 8.1).

var
  H, X: Int64;
begin
  X := 42;
  H := HashCombine(7, X);
  WriteLn(HashCombine(H, X) <> H);
end.
