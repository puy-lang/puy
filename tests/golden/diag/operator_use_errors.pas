program OperatorUseErrors;

// The errors of using operators (objects.md 11.1, 11.3, 11.4): an operator called by its name
// (E0700), operands no operator takes (E0735), two operators taking them equally well, one from
// each operand's type (E0734), `Implicit` declared by both types (E0734), `<>` of a type with
// only `Equal` (E0703, also for a reference counted class), `Inc` with a second argument
// (E0705), a condition that only `Implicit` would make `Boolean` (E0702), a unary operator the
// type does not declare and a type without operators (E0703).

type
  TY = class;

  TX = class(TRefObject)
    N: Integer;
    class operator Add(const A: TX; const B: TY): Integer;
    class operator Implicit(const A: TX): TY;
  end;

  TY = class(TRefObject)
    N: Integer;
    class operator Add(const A: TX; const B: TY): Integer;
    class operator Implicit(const A: TX): TY;
  end;

  TA = record
    N: Integer;
    class operator Equal(const A, B: TA): Boolean;
    function GetHashCode: Int64;
    class operator Implicit(const A: TA): Boolean;
    class operator Inc(const A: TA): TA;
  end;

  TB = record
    N: Integer;
  end;

  TR = class(TRefObject)
    class operator Equal(const A, B: TR): Boolean;
    function GetHashCode: Int64; override;
  end;

class operator TX.Add(const A: TX; const B: TY): Integer;
begin
  Result := A.N + B.N;
end;

class operator TX.Implicit(const A: TX): TY;
begin
  Result := TY.Create;
end;

class operator TY.Add(const A: TX; const B: TY): Integer;
begin
  Result := A.N - B.N;
end;

class operator TY.Implicit(const A: TX): TY;
begin
  Result := nil;
end;

class operator TA.Equal(const A, B: TA): Boolean;
begin
  Result := A.N = B.N;
end;

function TA.GetHashCode: Int64;
begin
  Result := N;
end;

class operator TA.Implicit(const A: TA): Boolean;
begin
  Result := A.N <> 0;
end;

class operator TA.Inc(const A: TA): TA;
begin
  Result := A;
end;

class operator TR.Equal(const A, B: TR): Boolean;
begin
  Result := True;
end;

function TR.GetHashCode: Int64;
begin
  Result := 0;
end;

var
  X: TX;
  Y: TY;
  A, A2: TA;
  B: TB;
  R1, R2: TR;
  N: Integer;
  Flag: Boolean;
begin
  X := TX.Create;
  Y := TY.Create;
  A := Default(TA);
  A2 := Default(TA);
  B := Default(TB);
  R1 := nil;
  R2 := nil;
  N := TX.Add(X, Y);
  N := X + 'text';
  N := X + Y;
  Y := X;
  Flag := A <> A2;
  Flag := R1 <> R2;
  Inc(A, 2);
  if A then
    WriteLn('set');
  A2 := -A;
  Flag := B = B;
  Flag := A;
  WriteLn(N, Flag);
end.
