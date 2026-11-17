program OperatorMoreErrors;

// More errors of operator declarations (objects.md 11.1, 11.2, 11.4): `Initialize` in a
// reference counted class is not an operator name (E0755), `Finalize` and `Assign` of a record
// are custom managed records (E3014), an `Explicit` from a type to itself repeats a built-in
// conversion, and a binary operator whose declaring type is only its result is E0755.

type
  TRef = class(TRefObject)
    class operator Initialize(out A: TRef);
  end;

  TV = record
    X: Integer;
    class operator Finalize(var A: TV);
    class operator Assign(var A: TV; const B: TV);
    class operator Explicit(const A: TV): TV;
    class operator Add(A, B: Integer): TV;
  end;

begin
end.
