program DistinctOverload;

// Overload resolution with distinct types (types.md 1.3, semantics.md 4.7): stage 0.5 (another
// type of the same root, one of them a distinct type) comes after stage 0 and before stage 1,
// and any other pair is ranked as the roots are. A user's operator is chosen the same way (the
// distinct type of the operand counts); the built-in operations take the root.

type
  TUserId = type Int64;
  TOrderId = type Int64;

  TTotal = record
    V: Int64;
    class operator Add(const A: TTotal; B: TUserId): TTotal;
    class operator Add(const A: TTotal; B: Int64): TTotal;
  end;

class operator TTotal.Add(const A: TTotal; B: TUserId): TTotal;
begin
  Result := Default(TTotal);
  Result.V := A.V + B * 100;
  WriteLn('Add(TTotal; TUserId) ', Result.V);
end;

class operator TTotal.Add(const A: TTotal; B: Int64): TTotal;
begin
  Result := Default(TTotal);
  Result.V := A.V + B;
  WriteLn('Add(TTotal; Int64) ', Result.V);
end;

procedure P(N: Int64); overload;
begin
  WriteLn('P(Int64) ', N);
end;

procedure P(N: Integer); overload;
begin
  WriteLn('P(Integer) ', N);
end;

procedure Q(Id: TUserId); overload;
begin
  WriteLn('Q(TUserId) ', Id);
end;

procedure Q(N: Int64); overload;
begin
  WriteLn('Q(Int64) ', N);
end;

procedure R(Id: TUserId); overload;
begin
  WriteLn('R(TUserId) ', Id);
end;

procedure R(N: Integer); overload;
begin
  WriteLn('R(Integer) ', N);
end;

procedure D(X: Double); overload;
begin
  WriteLn('D(Double) ', X:0:1);
end;

procedure D(S: string); overload;
begin
  WriteLn('D(string) ', S);
end;

var
  T: TTotal;
  Id: TUserId;
  Order: TOrderId;
  N: Int64;
  I: Integer;
begin
  Id := 1;
  Order := 2;
  N := 3;
  I := 4;
  // Stage 0.5 before stage 1 (`TUserId` to `Int64`, not to `Integer`)
  P(Id);
  // Stage 0 before stage 0.5
  Q(Id);
  Q(N);
  // `Id + 1` is of the root type
  Q(Id + 1);
  // Stage 0.5 between two distinct types of the same root
  R(Order);
  // `Int64` to `TUserId` is stage 0.5, to `Integer` stage 2
  R(N);
  // `Integer` to `Integer` is stage 0
  R(I);
  // A distinct type to `Double` is stage 2, as its root
  D(Id);
  // A user's operator: stage 0 for the distinct type, and `Id + 1` is of the root type
  T := Default(TTotal);
  T := T + Id;
  T := T + N;
  T := T + (Id + 1);
end.
