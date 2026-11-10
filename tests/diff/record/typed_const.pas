program TypedConst;

// Typed constants of records and static arrays (types.md chapter 4). Writing to a copy of
// a constant leaves the constant alone.

type
  TPoint = record
    X, Y: Integer;
  end;
  TNamed = record
    Name: string;
    At: TPoint;
  end;
  TDigits = array[0..4] of Integer;
  TMatrix = array[1..2, 1..2] of Integer;

const
  Origin: TPoint = (X: 0; Y: 0);
  Home: TNamed = (Name: 'home'; At: (X: 3; Y: 4));
  Digits: TDigits = (1, 2, 3, 4, 5);
  Identity: TMatrix = ((1, 0), (0, 1));
  Names: array[1..3] of string = ('one', 'two', 'three');

function Sum(const D: TDigits): Integer;
var
  I: Integer;
begin
  Result := 0;
  for I := Low(D) to High(D) do
    Result := Result + D[I];
end;

procedure Show;
const
  Corner: TPoint = (X: 9; Y: 9);
var
  P: TPoint;
begin
  P := Corner;
  P.X := 1;
  WriteLn(Corner.X, ' ', P.X);
end;

var
  P: TPoint;
  N: TNamed;
  D: TDigits;
  I: Integer;
begin
  P := Origin;
  P.X := 5;
  WriteLn(Origin.X, ' ', P.X);
  N := Home;
  N.At.Y := 40;
  N.Name := 'away';
  WriteLn(Home.Name, ' ', Home.At.X, ' ', Home.At.Y, ' ', N.Name, ' ', N.At.Y);
  D := Digits;
  D[0] := 100;
  WriteLn(Sum(Digits), ' ', Sum(D), ' ', Digits[0]);
  WriteLn(Identity[1, 1], Identity[1, 2], Identity[2, 1], Identity[2, 2]);
  for I := 1 to 3 do
    Write(Names[I], ' ');
  WriteLn;
  Show;
  Show;
end.
