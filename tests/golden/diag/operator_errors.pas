program OperatorErrors;

// The errors of operator declarations (objects.md 11.1, 11.2, 11.4): a manually managed class,
// an interface and a helper cannot declare operators (E0755), and a declaration is E0755 when
// the name is not an operator's (with a fix for a close name), the number of parameters or the
// result is wrong, no parameter (or result, for a conversion) has the declaring type, a parameter
// is `var` or has a default value, or `Implicit` repeats a built-in conversion. A custom managed
// record is E3014, a virtual operator E0744, and two operators with the same parameters E0701
// (`LogicalAnd` and `BitwiseAnd` are the same operator; `Implicit` may differ by its result).

type
  TManual = class
    class operator Add(const A, B: TManual): TManual;
  end;

  IShape = interface
    class operator Add(const A, B: IShape): IShape;
  end;

  TVec = record
    X: Integer;
    class operator Multipy(const A, B: TVec): TVec;
    class operator Plus(const A, B: TVec): TVec;
    class operator Add(const A: TVec): TVec;
    class operator Negative(const A, B: TVec): TVec;
    class operator Equal(const A, B: TVec): Integer;
    class operator Inc(const A: TVec): Integer;
    class operator Multiply(A, B: Integer): Integer;
    class operator Implicit(N: Integer): Double;
    class operator Subtract(var A: TVec; const B: TVec): TVec;
    class operator Divide(const A: TVec; B: Integer = 1): TVec;
    class operator Implicit(const A: TVec): TVec;
    class operator Initialize(out A: TVec);
    class operator Positive(const A: TVec): TVec; virtual;
    class operator LogicalOr(const A, B: TVec): TVec;
    class operator BitwiseOr(const A, B: TVec): TVec;
    class operator Implicit(const A: TVec): Integer;
    class operator Implicit(const A: TVec): string;
    class operator Implicit(const A: TVec): Integer;
  end;

  TRef = class(TRefObject)
    class operator Implicit(const A: TRef): TRefObject;
  end;

  TNumHelper = record helper for Integer
    class operator Add(A, B: Integer): Integer;
  end;

class operator TVec.LogicalOr(const A, B: TVec): TVec;
begin
  Result := A;
end;

class operator TVec.Positive(const A: TVec): TVec;
begin
  Result := A;
end;

class operator TVec.Implicit(const A: TVec): Integer;
begin
  Result := A.X;
end;

class operator TVec.Implicit(const A: TVec): string;
begin
  Result := 'vec';
end;

begin
end.
