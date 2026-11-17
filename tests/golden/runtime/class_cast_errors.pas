program ClassCastErrors;

// Failed checks of `as` and of conversions by type name (objects.md 2.8, 2.9, 14.3, 14.4): an
// instance of another class is EInvalidCast for `as`, for `TCircle(S)` (checked in Puy) and for a
// class reference converted by type name, naming the two classes; the variable assigned keeps its
// value. `nil` is not any class for `is` and passes `as` and the conversions as `nil`. `is` on a
// freed instance is EAccessViolation, while `is` of an ancestor is `<> nil` and does not look at
// the instance. The last conversion leaves the exception unhandled, for the report.

type
  TShape = class
  end;
  TCircle = class(TShape)
  end;
  TSquare = class(TShape)
  end;
  TShapeClass = class of TShape;
  TCircleClass = class of TCircle;

procedure Try1(const What: string; S: TShape; K: TShapeClass; Step: Integer);
var
  C: TCircle;
  CK: TCircleClass;
begin
  C := nil;
  CK := nil;
  try
    case Step of
      0: C := S as TCircle;
      1: C := TCircle(S);
      2: CK := TCircleClass(K);
      3: WriteLn(S is TCircle);
      4: WriteLn(S is TShape);
    end;
    WriteLn(What, ': no error');
  except
    on E: Exception do
      WriteLn(What, ': ', E.ClassName, ': ', E.Message);
  end;
  WriteLn(What, ': ', C = nil, ' ', CK = nil);
end;

var
  S: TShape;
  Q: TSquare;
  K: TShapeClass;
begin
  S := TSquare.Create;
  Try1('as', S, nil, 0);
  Try1('cast', S, nil, 1);
  Try1('class reference', nil, TSquare, 2);
  Try1('class reference to TShape', nil, TShape, 2);
  Try1('nil is', nil, nil, 3);
  Try1('nil as', nil, nil, 0);
  Try1('nil cast', nil, nil, 1);
  Try1('nil class reference', nil, nil, 2);
  S.Free;
  Try1('freed is', S, nil, 3);
  Try1('freed is ancestor', S, nil, 4);
  Try1('freed as', S, nil, 0);

  S := TCircle.Create;
  Q := nil;
  try
    Q := S as TSquare;
  except
    on E: Exception do
      WriteLn(E.Message);
  end;
  WriteLn(Q = nil, ' ', S.ClassName);
  S.Free;
  K := TShape;
  K := TCircleClass(K);
end.
