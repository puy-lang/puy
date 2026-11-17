program RecordMethodSelf;

// `Self` of a record method (objects.md chapter 6) called on a field or an element is taken out
// and put back like a `var` argument: touching `A[I]` through another path during the call is an
// `EAliasError`, and a record field of a class instance (`O.Pos.Offset(…)`) is lent from the
// instance, which may not be freed during the call. A method called on a `const` parameter, a
// function's result or a loop variable writes to a copy: the parameter itself does not change.

uses
  System.SysUtils;

type
  TPoint = record
    X, Y: Integer;
    procedure Offset(DX, DY: Integer);
    procedure TouchArray;
    procedure TouchHolder;
  end;

  THolder = class
  public
    Pos: TPoint;
  end;

var
  A: array of TPoint;
  O: THolder;

procedure TPoint.Offset(DX, DY: Integer);
begin
  X := X + DX;
  Y := Y + DY;
end;

procedure TPoint.TouchArray;
begin
  X := 1;
  try
    WriteLn('element: ', A[0].X);
  except
    on E: EAliasError do
      WriteLn('element: ', E.Message);
  end;
  WriteLn('other element: ', A[1].X);
end;

procedure TPoint.TouchHolder;
begin
  Y := 2;
  try
    WriteLn('field: ', O.Pos.Y);
  except
    on E: EAliasError do
      WriteLn('field: ', E.Message);
  end;
  O.Free;
end;

procedure ShowConst(const P: TPoint);
begin
  P.Offset(100, 100);
  WriteLn('const: ', P.X, ' ', P.Y);
end;

function Make: TPoint;
begin
  Result := Default(TPoint);
  Result.X := 9;
end;

var
  P: TPoint;
begin
  SetLength(A, 2);
  A[0].Offset(3, 4);
  WriteLn(A[0].X, ' ', A[0].Y);
  A[0].TouchArray;
  WriteLn('after: ', A[0].X);
  O := THolder.Create;
  O.Pos.Offset(7, 8);
  WriteLn(O.Pos.X, ' ', O.Pos.Y);
  try
    O.Pos.TouchHolder;
  except
    on E: EAccessViolation do
      WriteLn('freed: ', E.Message);
  end;
  P := Default(TPoint);
  P.X := 5;
  ShowConst(P);
  WriteLn('caller: ', P.X);
  Make.Offset(1, 1);
  for P in A do
    P.Offset(1000, 0);
  WriteLn('loop: ', A[0].X, ' ', A[1].X);
end.
