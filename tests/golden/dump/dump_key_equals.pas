program DumpKeyEquals;

// KeyEquals / KeyHash written by the user (equality.md 4, 9): the `hir` dump shows each call as
// `call KeyEquals<…>` with the type argument written or inferred, ` via fN` for the generated
// function of a record, and the generated `<eq:…>` / `<hash:…>` under `.generated`; in a generic
// body the call keeps `T`, and the instantiation shows the type it was made with.

type
  TPoint = record
    X, Y: Integer;
  end;

function Same<T>(const A, B: T): Boolean;
begin
  Result := KeyEquals<T>(A, B);
end;

var
  P, Q: TPoint;
  B: Boolean;
  H: Int64;
begin
  B := KeyEquals<TPoint>(P, Q);
  H := KeyHash(P);
  B := KeyEquals('a', 'b');
  B := Same(1, 2);
end.
