program TArrayDeclared;

// A `TArray` declared by the program hides the built-in one, so `<` cannot follow it
// (types.md chapter 1).

type
  TArray = record
    N: Integer;
  end;

var
  A: TArray<Integer>;                   // E0400
  R: TArray;
begin
  R.N := 1;
  WriteLn(R.N);
end.
