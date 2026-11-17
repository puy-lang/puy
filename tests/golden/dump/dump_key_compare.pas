program DumpKeyCompare;

// KeyCompare written by the user (collections.md 10.2): the `hir` dump shows each call as
// `call KeyCompare<…>`, with ` via fN` for a record with its own LessThan, whose generated
// `<cmp:…>` (`if A < B then -1 else if B < A then 1 else 0` over the operator) is shown under
// `.generated`; in a generic body the call keeps `T`.

type
  TVersion = record
    Major: Integer;
    class operator LessThan(const A, B: TVersion): Boolean;
  end;

class operator TVersion.LessThan(const A, B: TVersion): Boolean;
begin
  Result := A.Major < B.Major;
end;

function Order<T>(const A, B: T): Integer;
begin
  Result := KeyCompare(A, B);
end;

var
  V: TVersion;
begin
  V.Major := 1;
  WriteLn(KeyCompare(V, V));
  WriteLn(KeyCompare<Double>(1.5, 2.5));
  WriteLn(Order(1, 2));
end.
