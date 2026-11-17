program RecordMethods;

// Methods of records (objects.md chapter 6): `Self` of a method called on a variable is the
// variable, so what the method writes reaches the caller; a constructor runs on a new value; a
// `static` class function needs no value; properties read and write fields or call methods; a
// method called on a `const` parameter or on a function's result writes to a copy.

type
  TPoint = record
  private
    function GetSum: Integer;
    procedure SetSum(Value: Integer);
  public
    X, Y: Integer;
    constructor Create(AX, AY: Integer);
    function Length2: Integer;
    procedure Offset(DX, DY: Integer);
    procedure Scale(K: Integer);
    procedure Reset;
    function Moved(DX: Integer): Integer;
    class function Origin: TPoint; static;
    property Sum: Integer read GetSum write SetSum;
    property PX: Integer read X write X;
  end;

constructor TPoint.Create(AX, AY: Integer);
begin
  X := AX;
  Y := AY;
end;

function TPoint.Length2: Integer;
begin
  Result := X * X + Y * Y;
end;

procedure TPoint.Offset(DX, DY: Integer);
begin
  X := X + DX;
  Self.Y := Self.Y + DY;
end;

procedure TPoint.Scale(K: Integer);
begin
  X := X * K;
  Y := Y * K;
end;

procedure TPoint.Reset;
begin
  Self := TPoint.Create(0, 0);
end;

function TPoint.Moved(DX: Integer): Integer;
begin
  Offset(DX, 0);
  Result := X;
end;

class function TPoint.Origin: TPoint;
begin
  Result := TPoint.Create(0, 0);
end;

function TPoint.GetSum: Integer;
begin
  Result := X + Y;
end;

procedure TPoint.SetSum(Value: Integer);
begin
  // Puts the difference into X; calls another method on `Self`
  Offset(Value - GetSum, 0);
end;

function MakePoint(AX, AY: Integer): TPoint;
begin
  Result := TPoint.Create(AX, AY);
end;

procedure ShowConst(const P: TPoint);
begin
  WriteLn('const: ', P.Length2, ' ', P.Sum, ' ', P.Moved(10));
end;

procedure Move(var P: TPoint);
begin
  P.Offset(1, 1);
end;

var
  P, Q: TPoint;
begin
  P := TPoint.Create(3, 4);
  WriteLn(P.X, ' ', P.Y, ' ', P.Length2);
  P.Offset(1, 2);
  WriteLn(P.X, ' ', P.Y);
  Q := P;
  Q.Scale(10);
  WriteLn(P.X, ' ', Q.X);
  Move(P);
  WriteLn(P.X, ' ', P.Y);
  WriteLn(P.Sum);
  P.Sum := 100;
  WriteLn(P.X, ' ', P.Y, ' ', P.Sum);
  P.PX := 7;
  WriteLn(P.PX);
  ShowConst(P);
  WriteLn(P.X);
  WriteLn(MakePoint(5, 12).Length2, ' ', MakePoint(5, 12).Moved(3));
  WriteLn(TPoint.Origin.Length2);
  WriteLn(TPoint.Create(2, 3).Sum);
  P.Reset;
  WriteLn(P.X, ' ', P.Y);
end.
