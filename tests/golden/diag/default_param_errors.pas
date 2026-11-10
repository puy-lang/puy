// Errors in default parameter values (semantics.md 4.8): E0738 for `var` / `out`, for a group
// of several names, for the order and for array and record types; E0713 for a value that is
// not constant; E0705 with a range of counts; E0723 when a body changes a default value.

program DefaultParamErrors;

type
  TPoint = record
    X, Y: Integer;
  end;

procedure V(var X: Integer = 0);
begin
end;

procedure O(out X: Integer = 0);
begin
  X := 1;
end;

procedure Two(A, B: Integer = 0);
begin
end;

procedure Order(A: Integer = 0; B: Integer);
begin
end;

procedure Arr(A: TArray<Integer> = nil);
begin
end;

procedure Rec(P: TPoint = 0);
begin
end;

var
  G: Integer;

procedure NotConst(A: Integer = G);
begin
end;

procedure Mismatch(A: Integer = 1); forward;
procedure Same(A: Integer = 1); forward;
procedure Left(A: Integer = 1); forward;

procedure Mismatch(A: Integer = 2);
begin
end;

procedure Same(A: Integer = 1);
begin
end;

procedure Left(A: Integer);
begin
end;

procedure Range(A: Integer; B: Integer = 0; C: Integer = 0);
begin
end;

procedure TypeErr(A: Integer = 'x');
begin
end;

begin
  G := 1;
  Range(1, 2, 3, 4);
  Range();
  Same;
  Left;
  TypeErr;
end.
