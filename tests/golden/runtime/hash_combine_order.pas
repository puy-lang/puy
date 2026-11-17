program HashCombineOrder;

// HashCombine mixes X into H so that the order matters, and gives the same value for the same
// arguments in the same process (equality.md 4, 7.2). The values themselves change between
// processes, so only their comparisons are printed.

var
  A, B, H1, H2: Int64;
  I: Integer;
begin
  A := HashCombine(1, 2);
  B := HashCombine(2, 1);
  WriteLn(A <> B);
  WriteLn(A = HashCombine(1, 2));
  // Nested calls keep each argument apart
  H1 := HashCombine(HashCombine(0, 10), 20);
  H2 := HashCombine(HashCombine(0, 20), 10);
  WriteLn(H1 <> H2);
  WriteLn(H1 = HashCombine(HashCombine(0, 10), 20));
  // Narrower integer arguments widen to Int64
  I := -1;
  WriteLn(HashCombine(I, I) = HashCombine(-1, -1));
  WriteLn(HashCombine(I, 0) <> HashCombine(0, I));
end.
