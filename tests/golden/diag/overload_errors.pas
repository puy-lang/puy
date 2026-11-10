// Resolution errors of overloaded calls (semantics.md 4.7): E0734 lists the candidates of the
// stage that does not decide, E0735 lists every candidate and the argument types, and an
// argument with an error type reports nothing more unless the argument count picks one overload.

program OverloadErrors;

procedure P(X: Integer); overload;
begin
end;

procedure P(X: Int64); overload;
begin
end;

procedure P(const S: string); overload;
begin
end;

procedure Q(A: Integer; B: Int64); overload;
begin
end;

procedure Q(A: Int64; B: Integer); overload;
begin
end;

function F(N: Integer = 1): Integer; overload;
begin
  Result := N;
end;

function F(const S: string = ''): Integer; overload;
begin
  Result := Length(S);
end;

procedure T(A: Integer); overload;
begin
end;

procedure T(A, B: Integer); overload;
begin
end;

procedure R(var X: Integer); overload;
begin
end;

procedure R(var X: string); overload;
begin
end;

var
  B: Byte;
  D: Double;
  W: Word;
begin
  B := 1;
  D := 1.5;
  W := 2;
  P(B);
  P(D);
  P(1, 2);
  P();
  Q(1, 2);
  WriteLn(F);
  R(W);
  P(Undefined);
  P(B + 'x');
  T(Undefined, 'x');
  P;
end.
