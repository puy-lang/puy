program RecordOperators;

// Operators of a record (objects.md 11.1-11.4): arithmetic with the operand types mixed, unary
// minus, comparisons, `and` / `xor` on the record, `in` with the record on the right, `Inc` / `Dec`,
// `Implicit` (an assignment, an argument, a result, an operand converted for an operator, an
// integer constant through `Int64`) and `Explicit` (a conversion by the type's name). `A + B * C`
// keeps the usual precedence.

uses
  System.SysUtils;

type
  TVec = record
    X, Y: Integer;
    class operator Add(const A, B: TVec): TVec;
    class operator Subtract(const A, B: TVec): TVec;
    class operator Multiply(const A: TVec; K: Integer): TVec;
    class operator Multiply(K: Integer; const A: TVec): TVec;
    class operator Negative(const A: TVec): TVec;
    class operator Equal(const A, B: TVec): Boolean;
    function GetHashCode: Int64;
    class operator NotEqual(const A, B: TVec): Boolean;
    class operator LessThan(const A, B: TVec): Boolean;
    class operator LogicalAnd(const A, B: TVec): TVec;
    class operator LogicalXor(const A, B: TVec): TVec;
    class operator Inc(const A: TVec): TVec;
    class operator Dec(const A: TVec): TVec;
    class operator Implicit(N: Int64): TVec;
    class operator Implicit(const A: TVec): string;
    class operator Explicit(const A: TVec): Integer;
  end;

  TBox = record
    Lo, Hi: Integer;
    class operator In(N: Integer; const B: TBox): Boolean;
  end;

function V(X, Y: Integer): TVec;
begin
  Result := Default(TVec);
  Result.X := X;
  Result.Y := Y;
end;

class operator TVec.Add(const A, B: TVec): TVec;
begin
  Result := V(A.X + B.X, A.Y + B.Y);
end;

class operator TVec.Subtract(const A, B: TVec): TVec;
begin
  Result := V(A.X - B.X, A.Y - B.Y);
end;

class operator TVec.Multiply(const A: TVec; K: Integer): TVec;
begin
  Result := V(A.X * K, A.Y * K);
end;

class operator TVec.Multiply(K: Integer; const A: TVec): TVec;
begin
  Result := V(K * A.X, K * A.Y + 1);
end;

class operator TVec.Negative(const A: TVec): TVec;
begin
  Result := V(-A.X, -A.Y);
end;

class operator TVec.Equal(const A, B: TVec): Boolean;
begin
  Result := (A.X = B.X) and (A.Y = B.Y);
end;

function TVec.GetHashCode: Int64;
begin
  Result := Int64(X) * 31 + Y;
end;

class operator TVec.NotEqual(const A, B: TVec): Boolean;
begin
  Result := not (A = B);
end;

class operator TVec.LessThan(const A, B: TVec): Boolean;
begin
  Result := A.X * A.X + A.Y * A.Y < B.X * B.X + B.Y * B.Y;
end;

class operator TVec.LogicalAnd(const A, B: TVec): TVec;
begin
  Result := V(A.X and B.X, A.Y and B.Y);
end;

class operator TVec.LogicalXor(const A, B: TVec): TVec;
begin
  Result := V(A.X xor B.X, A.Y xor B.Y);
end;

class operator TVec.Inc(const A: TVec): TVec;
begin
  Result := V(A.X + 1, A.Y + 1);
end;

class operator TVec.Dec(const A: TVec): TVec;
begin
  Result := V(A.X - 1, A.Y - 1);
end;

class operator TVec.Implicit(N: Int64): TVec;
begin
  Result := V(N, N);
end;

class operator TVec.Implicit(const A: TVec): string;
begin
  Result := '(' + IntToStr(A.X) + ', ' + IntToStr(A.Y) + ')';
end;

class operator TVec.Explicit(const A: TVec): Integer;
begin
  Result := A.X * 100 + A.Y;
end;

class operator TBox.In(N: Integer; const B: TBox): Boolean;
begin
  Result := (N >= B.Lo) and (N <= B.Hi);
end;

function Show(const S: string): string;
begin
  Result := '[' + S + ']';
end;

function Make(N: Int64): TVec;
begin
  Result := N;
end;

var
  A, B, C: TVec;
  S: string;
  Box: TBox;
begin
  A := V(1, 2);
  B := V(3, 4);
  C := A + B * 2;
  S := C;
  WriteLn(S);
  S := 3 * A - B;
  WriteLn(S);
  S := -A;
  WriteLn(S);
  WriteLn(A = B, ' ', A <> B, ' ', A = V(1, 2), ' ', A < B, ' ', B < A);
  S := (A and B) xor V(7, 7);
  WriteLn(S);
  C := 5;
  WriteLn(Show(C));
  WriteLn(Show(A + 10));
  WriteLn(Show(Make(-3)));
  Inc(C);
  WriteLn(Show(C));
  Dec(C);
  Dec(C);
  WriteLn(Show(C));
  WriteLn(Integer(B));
  Box.Lo := 2;
  Box.Hi := 5;
  WriteLn(1 in Box, ' ', 3 in Box, ' ', 5 in Box);
end.
