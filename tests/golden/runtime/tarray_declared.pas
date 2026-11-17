program TArrayDeclared;

// A `TArray` without type parameters declared by the program is another name than the built-in
// `TArray<T>`, which can still be written unqualified (generics.md 2.3, 6.2).

type
  TArray = record
    N: Integer;
  end;

var
  A: TArray<Integer>;
  B: System.TArray<string>;
  R: TArray;
begin
  A := [1, 2, 3];
  B := ['x'];
  R.N := Length(A);
  WriteLn(R.N, ' ', Length(B), ' ', Length(Default(TArray<Byte>)));
end.
