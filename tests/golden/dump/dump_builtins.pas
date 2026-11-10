program DumpBuiltins;

// The hir and bytecode dumps of the M2b System built-ins (semantics.md 9.1, bytecode.md 5.2):
// insert / delete statements, the `arr` concatenation, Odd / UpCase, the `ExitCode` global
// (kind `exitcode`) and `Halt` without a code reading it.

var
  A: TArray<Integer>;
  S: string;
  U: UTF8String;
  C: Char;
begin
  A := A + [1];
  Insert(2, A, 0);
  Insert(A, A, 1);
  Delete(A, 0, 1);
  S := Concat('a', S, 'b');
  Insert('x', S, 1);
  Delete(S, 1, 1);
  U := 'u';
  Insert(U, U, 1);
  Delete(U, 1, 1);
  C := UpCase(S[1]);
  if Odd(Length(A)) then
    ExitCode := 2;
  Halt;
end.
