program GenericSystemQualifiedErrors;

// A name qualified with `System` and written with type arguments (generics.md 6.2, 2.3): a type
// that is not generic is E0761, a name `System` does not declare is E0700, and the number of type
// arguments is checked as without the qualification, also for `TArray` in a type and in an
// expression (the parser takes any number after a qualified name).

var
  A: System.Integer<Byte>;
  B: System.TFoo<Byte>;
  C: System.TProc<Byte, Byte, Byte, Byte, Byte>;
  D: System.TArray<Byte, Byte>;
  E: TArray<Byte>;
begin
  E := Default(System.TArray<Byte, Byte>);
end.
