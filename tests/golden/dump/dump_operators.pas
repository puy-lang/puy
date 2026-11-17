program DumpOperators;

// The dumps of operators (objects.md 15.6): `.method fN Add operator` in the type, `operator`
// for an operator called by an expression, `convert-op` for `Implicit` / `Explicit`, and `binary`
// for `=` with `nil`, which compares the references; the functions are named after the type.
// An operator name written in lower case (`explicit`) is the same operator.

type
  TVec = record
    X: Integer;
    class operator Add(const A, B: TVec): TVec;
    class operator Implicit(N: Integer): TVec;
    class operator explicit(const A: TVec): Integer;
  end;

  TNode = class(TRefObject)
    class operator Equal(const A, B: TNode): Boolean;
    function GetHashCode: Int64; override;
  end;

class operator TVec.Add(const A, B: TVec): TVec;
begin
  Result := Default(TVec);
  Result.X := A.X + B.X;
end;

class operator TVec.Implicit(N: Integer): TVec;
begin
  Result := Default(TVec);
  Result.X := N;
end;

class operator TVec.explicit(const A: TVec): Integer;
begin
  Result := A.X;
end;

class operator TNode.Equal(const A, B: TNode): Boolean;
begin
  Result := True;
end;

function TNode.GetHashCode: Int64;
begin
  Result := 0;
end;

var
  V: TVec;
  N: TNode;
begin
  V := 1;
  V := V + 2;
  WriteLn(Integer(V));
  N := TNode.Create;
  WriteLn(N = N, ' ', N = nil);
end.
