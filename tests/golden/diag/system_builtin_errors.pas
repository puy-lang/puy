program SystemBuiltinErrors;

// Diagnostics of the M2b System built-ins (semantics.md 5.3, types.md 8.6): SizeOf of a type
// without a memory layout (E3200, planned with pointers), `[…] + […]` (E0729), Concat with no
// argument or a value it cannot join (E0705, E0702), and the arguments of Insert / Delete
// (E0705, E0706, E0702; a constructor source is an array of the target's type, so its
// elements must be the element type).

type
  TPoint = record
    X, Y: Integer;
  end;
  TArr = array[0..2] of Integer;
  TSet = set of Byte;
  TInts = array of Integer;

var
  S: string;
  A: TArray<Integer>;
  P: TPoint;
  N: Integer;
  M: array of TInts = nil;
begin
  N := SizeOf(string);
  N := SizeOf(A);
  N := SizeOf(TArr);
  N := SizeOf(P);
  N := SizeOf(TSet);
  A := [1] + [2];
  S := Concat();
  S := Concat('a', 1);
  A := Concat(A, ['x']);
  Insert('a', S);
  Insert('a', 'b', 1);
  Insert('a', N, 1);
  Insert('a', A, 0);
  Delete(S, 'a', 1);
  N := Ord(Odd('a'));
  N := Ord(UpCase(1));
  Insert([1, 2], M, 0);
end.
