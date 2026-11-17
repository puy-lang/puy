program OperatorNamesRuntime;

// The operator names the other cases do not run (objects.md 11.2): `/`, `div`, `mod`, `shl`,
// `shr`, unary `+` and `not`, `<=`, `>`, `>=`, and `in` declared by the left operand's type
// (`A in B` where only `A`'s type has `In`).

type
  TN = record
    V: Integer;
    class operator Divide(const A, B: TN): Double;
    class operator IntDivide(const A, B: TN): TN;
    class operator Modulus(const A, B: TN): TN;
    class operator LeftShift(const A: TN; K: Integer): TN;
    class operator RightShift(const A: TN; K: Integer): TN;
    class operator Positive(const A: TN): string;
    class operator LogicalNot(const A: TN): TN;
    class operator LessThanOrEqual(const A, B: TN): Boolean;
    class operator GreaterThan(const A, B: TN): Boolean;
    class operator GreaterThanOrEqual(const A, B: TN): Boolean;
    class operator In(const A: TN; const B: TArray<Integer>): Boolean;
  end;

function N(V: Integer): TN;
begin
  Result := Default(TN);
  Result.V := V;
end;

class operator TN.Divide(const A, B: TN): Double;
begin
  Result := A.V / B.V;
end;

class operator TN.IntDivide(const A, B: TN): TN;
begin
  Result := N(A.V div B.V);
end;

class operator TN.Modulus(const A, B: TN): TN;
begin
  Result := N(A.V mod B.V);
end;

class operator TN.LeftShift(const A: TN; K: Integer): TN;
begin
  Result := N(A.V shl K);
end;

class operator TN.RightShift(const A: TN; K: Integer): TN;
begin
  Result := N(A.V shr K);
end;

class operator TN.Positive(const A: TN): string;
begin
  Result := 'positive ' + A.V.ToString;
end;

class operator TN.LogicalNot(const A: TN): TN;
begin
  Result := N(-A.V);
end;

class operator TN.LessThanOrEqual(const A, B: TN): Boolean;
begin
  WriteLn('  <= called');
  Result := A.V <= B.V;
end;

class operator TN.GreaterThan(const A, B: TN): Boolean;
begin
  WriteLn('  > called');
  Result := A.V > B.V;
end;

class operator TN.GreaterThanOrEqual(const A, B: TN): Boolean;
begin
  WriteLn('  >= called');
  Result := A.V >= B.V;
end;

class operator TN.In(const A: TN; const B: TArray<Integer>): Boolean;
var
  I: Integer;
begin
  Result := False;
  for I in B do
    if I = A.V then
      Result := True;
end;

var
  A, B: TN;
  Arr: TArray<Integer>;
begin
  A := N(17);
  B := N(5);
  WriteLn('/ ', A / B:0:1);
  WriteLn('div ', (A div B).V);
  WriteLn('mod ', (A mod B).V);
  WriteLn('shl ', (A shl 2).V);
  WriteLn('shr ', (A shr 2).V);
  WriteLn(+A);
  WriteLn('not ', (not A).V);
  WriteLn('<= ', A <= B);
  WriteLn('> ', A > B);
  WriteLn('>= ', B >= B);
  Arr := [1, 5, 9];
  WriteLn('in ', A in Arr, ' ', B in Arr);
end.
