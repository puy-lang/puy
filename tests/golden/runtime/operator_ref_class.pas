program OperatorRefClass;

// Operators of a reference counted class (objects.md 11.1, 11.3): `=` / `<>` call `Equal` /
// `NotEqual`, but comparing with the literal `nil` always compares the references (as `Assigned`
// does), while a variable holding `nil` is passed to the operator. An operator declared by an
// ancestor applies to a descendant, `Implicit` converts an integer, and `Inc` replaces the
// instance a variable holds. A class without `Equal` compares the references.

type
  TNum = class(TRefObject)
    V: Integer;
    constructor Create(AV: Integer);
    class operator Add(const A, B: TNum): TNum;
    class operator Equal(const A, B: TNum): Boolean;
    function GetHashCode: Int64; override;
    class operator NotEqual(const A, B: TNum): Boolean;
    class operator Implicit(N: Integer): TNum;
    class operator Inc(const A: TNum): TNum;
  end;

  TNamedNum = class(TNum)
    Name: string;
  end;

  TPlain = class(TRefObject)
  end;

constructor TNum.Create(AV: Integer);
begin
  V := AV;
end;

class operator TNum.Add(const A, B: TNum): TNum;
begin
  Result := TNum.Create(A.V + B.V);
end;

class operator TNum.Equal(const A, B: TNum): Boolean;
begin
  if (A = nil) or (B = nil) then
  begin
    WriteLn('  Equal got nil');
    Exit(Assigned(A) = Assigned(B));
  end;
  Result := A.V = B.V;
end;

function TNum.GetHashCode: Int64;
begin
  Result := V;
end;

class operator TNum.NotEqual(const A, B: TNum): Boolean;
begin
  Result := not (A = B);
end;

class operator TNum.Implicit(N: Integer): TNum;
begin
  Result := TNum.Create(N);
end;

class operator TNum.Inc(const A: TNum): TNum;
begin
  Result := TNum.Create(A.V + 1);
end;

var
  A, B, Z: TNum;
  N: TNamedNum;
  Old: TNum;
  P, Q: TPlain;
begin
  A := TNum.Create(2);
  B := 2;
  WriteLn('A = B: ', A = B, ', A <> B: ', A <> B);
  WriteLn('A = nil: ', A = nil, ', A <> nil: ', A <> nil, ', nil = A: ', nil = A);
  Z := nil;
  WriteLn('Z = nil: ', Z = nil, ', Assigned(Z): ', Assigned(Z));
  WriteLn('A = Z:');
  WriteLn('  ', A = Z);
  N := TNamedNum.Create(5);
  N.Name := 'five';
  WriteLn('N + A = ', (N + A).V, ', N = 5: ', N = 5);
  Old := A;
  Inc(A);
  WriteLn('Inc: ', A.V, ', the old one is kept by another variable: ', Old.V);
  P := TPlain.Create;
  Q := TPlain.Create;
  WriteLn('P = Q: ', P = Q, ', P = P: ', P = P);
end.
