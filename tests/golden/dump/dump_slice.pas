program DumpSlice;

// The `hir` and `bytecode` dumps of slices (semantics.md 9, bytecode.md 2.18): the four forms
// on a string, a `UTF8String`, a dynamic array, and a static array whose low bound is not 0.

type
  TFive = array[5..9] of Integer;

var
  S: string;
  U: UTF8String;
  D: TArray<Integer>;
  F: TFive;
begin
  S := 'abcdef';
  U := 'xyz';
  D := [1, 2, 3];
  F := Default(TFive);
  WriteLn(S[2..3], S[4..], S[..2], S[..]);
  WriteLn(U[2..]);
  D := D[1..];
  D := F[..6];
  WriteLn(Length(D));
end.
