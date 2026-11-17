program Vectors;

uses
  System.SysUtils;

// Records with methods: constructors, functions that return new values,
// procedures that change the record in place, static class functions and
// properties. Records stay values: assigning one copies it.

type
  TVector = record
  private
    FX, FY: Double;
    function GetLength: Double;
  public
    constructor Create(AX, AY: Double);
    class function Zero: TVector; static;
    function Add(const V: TVector): TVector;
    function Scale(Factor: Double): TVector;
    function Dot(const V: TVector): Double;
    procedure Normalize;
    function ToString: string;
    property X: Double read FX write FX;
    property Y: Double read FY write FY;
    property Length: Double read GetLength;
  end;

  TBox = record
    Min, Max: TVector;
    constructor Create(const P: TVector);
    function Contains(const P: TVector): Boolean;
    procedure Include(const P: TVector);
    function ToString: string;
  end;

constructor TVector.Create(AX, AY: Double);
begin
  FX := AX;
  FY := AY;
end;

class function TVector.Zero: TVector;
begin
  Result := TVector.Create(0, 0);
end;

function TVector.GetLength: Double;
begin
  Result := Sqrt(Sqr(FX) + Sqr(FY));
end;

function TVector.Add(const V: TVector): TVector;
begin
  Result := TVector.Create(FX + V.X, FY + V.Y);
end;

function TVector.Scale(Factor: Double): TVector;
begin
  Result := TVector.Create(FX * Factor, FY * Factor);
end;

function TVector.Dot(const V: TVector): Double;
begin
  Result := FX * V.X + FY * V.Y;
end;

// A procedure method changes the record it is called on.
procedure TVector.Normalize;
begin
  var L := Self.Length; // the property, not System.Length
  if L > 0 then
  begin
    FX := FX / L;
    FY := FY / L;
  end;
end;

function TVector.ToString: string;
begin
  Result := Format('(%.2f, %.2f)', [FX, FY]);
end;

constructor TBox.Create(const P: TVector);
begin
  Min := P;
  Max := P;
end;

function TBox.Contains(const P: TVector): Boolean;
begin
  Result := (P.X >= Min.X) and (P.X <= Max.X) and (P.Y >= Min.Y) and (P.Y <= Max.Y);
end;

// Grows the box so that it contains P. Min.X writes through the X property
// of the field Min.
procedure TBox.Include(const P: TVector);
begin
  if P.X < Min.X then Min.X := P.X;
  if P.X > Max.X then Max.X := P.X;
  if P.Y < Min.Y then Min.Y := P.Y;
  if P.Y > Max.Y then Max.Y := P.Y;
end;

function TBox.ToString: string;
begin
  Result := Min.ToString + ' .. ' + Max.ToString;
end;

var
  A, B, C: TVector;
  Points: TArray<TVector>;
  Box: TBox;

begin
  A := TVector.Create(3, 4);
  B := TVector.Create(-1, 2);
  WriteLn('A = ', A.ToString, ', length ', A.Length:0:2);
  WriteLn('A + B = ', A.Add(B).ToString);
  WriteLn('A * 0.5 = ', A.Scale(0.5).ToString);
  WriteLn('A . B = ', A.Dot(B):0:2);

  // C is a copy: normalizing it leaves A alone.
  C := A;
  C.Normalize;
  WriteLn('C = ', C.ToString, ', A = ', A.ToString);

  Points := [A, B, TVector.Create(2, -3)];
  Box := TBox.Create(TVector.Zero);
  for var P in Points do
    Box.Include(P);
  WriteLn('box ', Box.ToString);
  WriteLn('contains (1, 1): ', Box.Contains(TVector.Create(1, 1)));
  WriteLn('contains (5, 0): ', Box.Contains(TVector.Create(5, 0)));
end.
