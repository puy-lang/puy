program OperatorChoice;

// How operators are chosen and called (objects.md 11.2-11.4): the operands are evaluated left
// to right before the call, and an operator's `and` / `or` does not short-circuit; `BitwiseOr`
// serves `or`; `Trunc` / `Round` call the operators; an overload taking the argument by a
// built-in conversion wins over one that needs `Implicit` (stage 3), and `Implicit` is used when
// no other overload takes it; `T(X)` uses `Implicit` when there is no `Explicit`; `Implicit`
// also converts an element of an array constructor and the value of `Exit`.

type
  TFlag = record
    On: Boolean;
    class operator LogicalAnd(const A, B: TFlag): TFlag;
    class operator BitwiseOr(const A, B: TFlag): TFlag;
    class operator Trunc(const A: TFlag): Integer;
    class operator Round(const A: TFlag): Integer;
    class operator Implicit(B: Boolean): TFlag;
    class operator Implicit(const A: TFlag): string;
  end;

class operator TFlag.LogicalAnd(const A, B: TFlag): TFlag;
begin
  Result := Default(TFlag);
  Result.On := A.On and B.On;
end;

class operator TFlag.BitwiseOr(const A, B: TFlag): TFlag;
begin
  Result := Default(TFlag);
  Result.On := A.On or B.On;
end;

class operator TFlag.Trunc(const A: TFlag): Integer;
begin
  Result := Ord(A.On);
end;

class operator TFlag.Round(const A: TFlag): Integer;
begin
  Result := Ord(A.On) * 10;
end;

class operator TFlag.Implicit(B: Boolean): TFlag;
begin
  Result := Default(TFlag);
  Result.On := B;
end;

class operator TFlag.Implicit(const A: TFlag): string;
begin
  if A.On then
    Result := 'on'
  else
    Result := 'off';
end;

function Traced(const Name: string; B: Boolean): TFlag;
begin
  WriteLn('  eval ', Name);
  Result := B;
end;

procedure Show(const S: string); overload;
begin
  WriteLn('  string ', S);
end;

procedure Show(B: Boolean); overload;
begin
  WriteLn('  Boolean ', B);
end;

procedure Show(const F: TFlag; N: Integer); overload;
begin
  WriteLn('  flag ', string(F), ' ', N);
end;

function Pick(B: Boolean): TFlag;
begin
  Exit(not B);
end;

var
  F: TFlag;
  Flags: array of TFlag;
begin
  WriteLn('and:');
  F := Traced('a', False) and Traced('b', True);
  WriteLn('  ', string(F));
  WriteLn('or:');
  F := Traced('c', True) or Traced('d', False);
  WriteLn('  ', string(F));
  WriteLn('Trunc ', Trunc(F), ', Round ', Round(F));
  WriteLn('calls:');
  Show(True);
  Show(F);
  Show(True, 3);
  Flags := [True, F, False];
  WriteLn('array: ', string(Flags[0]), ' ', string(Flags[1]), ' ', string(Flags[2]));
  WriteLn('Exit: ', string(Pick(True)));
end.
