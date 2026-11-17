program ProcTypes;

// Procedural types (objects.md 9.2, 9.4, 9.5): a variable of a procedural type holds a routine
// written by its name and calls it with arguments and a result; an overloaded routine is chosen by
// the type it goes to; procedural values are passed as arguments (also to a `var` parameter),
// returned by functions, kept in record fields and array elements, and called from there; a
// value without parameters written without parentheses is called outside a value position;
// `Assigned` and `nil` tell whether a value holds a routine.

type
  TBinOp = function(A, B: Integer): Integer;
  TUnOp = function(A: Integer): Integer;
  TAction = procedure;
  TGetter = function: Integer;
  TLogger = procedure(const S: string);

  TOps = record
    Name: string;
    Op: TBinOp;
  end;

function Add(A, B: Integer): Integer;
begin
  Result := A + B;
end;

function Mul(A, B: Integer): Integer;
begin
  Result := A * B;
end;

function Twice(A: Integer): Integer; overload;
begin
  Result := A * 2;
end;

function Twice(A, B: Integer): Integer; overload;
begin
  Result := (A + B) * 2;
end;

procedure Hello;
begin
  WriteLn('hello');
end;

function Seven: Integer;
begin
  Result := 7;
end;

procedure Show(const S: string);
begin
  WriteLn('[', S, ']');
end;

function Fold(const A: array of Integer; Op: TBinOp; Start: Integer): Integer;
var
  I: Integer;
begin
  Result := Start;
  for I := 0 to High(A) do
    Result := Op(Result, A[I]);
end;

function Pick(Product: Boolean): TBinOp;
begin
  if Product then
    Result := Mul
  else
    Result := Add;
end;

procedure UseMul(var Op: TBinOp);
begin
  Op := Mul;
end;

procedure Run(Log: TLogger; const Msg: string);
begin
  if Assigned(Log) then
    Log(Msg)
  else
    WriteLn('no logger');
end;

var
  Op: TBinOp;
  Un: TUnOp;
  Act: TAction;
  Get: TGetter;
  Table: array[0..1] of TOps;
  Steps: array of TUnOp;
  I, N: Integer;
begin
  Op := Add;
  WriteLn(Op(2, 3));
  Op := Mul;
  WriteLn(Op(2, 3));
  Un := Twice;
  Op := Twice;
  WriteLn(Un(5), ' ', Op(5, 1));
  WriteLn(Fold([1, 2, 3, 4], Add, 0), ' ', Fold([1, 2, 3, 4], Mul, 1));
  WriteLn(Pick(True)(6, 7), ' ', Pick(False)(6, 7));
  Op := Add;
  UseMul(Op);
  WriteLn(Op(4, 4));
  Table[0].Name := 'add';
  Table[0].Op := Add;
  Table[1].Name := 'mul';
  Table[1].Op := Mul;
  for I := 0 to 1 do
    WriteLn(Table[I].Name, ' ', Table[I].Op(10, 3));
  SetLength(Steps, 2);
  Steps[0] := Twice;
  Steps[1] := Twice;
  N := 1;
  for I := 0 to High(Steps) do
    N := Steps[I](N);
  WriteLn(N);
  Act := Hello;
  Act;
  Act();
  Get := Seven;
  N := Get;
  WriteLn(N, ' ', Get + 1, ' ', Get() * 2);
  Run(Show, 'message');
  Run(nil, 'lost');
  WriteLn(Assigned(Op));
  Op := nil;
  WriteLn(Assigned(Op));
end.
