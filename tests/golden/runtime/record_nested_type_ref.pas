program RecordNestedTypeRef;

// A type nested in a record is named through the record's name as one nested in a class is
// (objects.md 2.2, generics.md 2.5): in a plain record, an instantiation of a generic record,
// an alias of that instantiation, after the type arguments (`TBox<Integer>.TCell`), a generic
// type nested in a record, a record nested in a class nested in a record, and in the heading of
// a nested record's method. Values of the forms naming the same type are assignable.

type
  TShape = record
  type
    TPoint = record
      X, Y: Integer;
      constructor Create(AX, AY: Integer);
      function Sum: Integer;
    end;
    TKind = (skCircle, skSquare);
    TPoints = array of TPoint;
    TPair<U> = record
      Left, Right: U;
    end;
    THolder = class
    type
      TTag = record
        Name: string;
      end;
    end;
  const
    Limit = 10;
  var
    Origin: TPoint;
    procedure Move(const P: TPoint);
  end;

  TBox<T> = record
  type
    TCell = record
      Value: T;
      class function Make(const A: T): TCell; static;
      function Describe: string;
    end;
  var
    Cell: TCell;
  end;

  TIntBox = TBox<Integer>;
  TIntCell = TIntBox.TCell;

constructor TShape.TPoint.Create(AX, AY: Integer);
begin
  X := AX;
  Y := AY;
end;

function TShape.TPoint.Sum: Integer;
begin
  Result := X + Y;
end;

procedure TShape.Move(const P: TPoint);
begin
  Origin := P;
end;

class function TBox<T>.TCell.Make(const A: T): TCell;
begin
  Result := Default(TCell);
  Result.Value := A;
end;

function TBox<T>.TCell.Describe: string;
begin
  Result := 'cell';
end;

function Total(const Points: TShape.TPoints): Integer;
var
  P: TShape.TPoint;
begin
  Result := 0;
  for P in Points do
    Result := Result + P.Sum;
end;

var
  S: TShape;
  P: TShape.TPoint;
  K: TShape.TKind;
  Points: TShape.TPoints;
  Pair: TShape.TPair<string>;
  Tag: TShape.THolder.TTag;
  A: TBox<Integer>.TCell;
  B: TIntBox.TCell;
  C: TIntCell;
  D: TBox<string>.TCell;
begin
  P := TShape.TPoint.Create(1, 2);
  S.Move(P);
  WriteLn(S.Origin.Sum, ' ', TShape.Limit);
  K := TShape.TKind.skSquare;
  WriteLn(Ord(K));
  Points := [TShape.TPoint.Create(3, 4), P];
  WriteLn(Total(Points));
  Pair.Left := 'l';
  Pair.Right := 'r';
  WriteLn(Pair.Left, Pair.Right);
  Tag.Name := 'tag';
  WriteLn(Tag.Name);
  A := TIntBox.TCell.Make(5);
  B := A;
  C := B;
  A := C;
  WriteLn(A.Value, ' ', B.Value, ' ', C.Value, ' ', C.Describe);
  D := TBox<string>.TCell.Make('s');
  WriteLn(D.Value);
end.
