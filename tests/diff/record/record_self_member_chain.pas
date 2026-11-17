program RecordSelfMemberChain;

// In a record's method, a field, property or `class var` of `Self` written without `Self.` is
// a value whose members can be read and written (objects.md chapter 6): `Min.X`, `Min.FX := V`,
// `Min.X := V`, `Box.V.FX`, a `var` argument `Min.FX`, a method `Min.Twice`, a property `P.FX`.
// A class's method does the same through its own property (`P.FX`).

type
  TV = record
    FX: Integer;
    function Twice: Integer;
    property X: Integer read FX write FX;
  end;

  TW = record
    V: TV;
  end;

  TB = record
    Min: TV;
    Box: TW;
    class var Shared: TV;
    function ReadX: Integer;
    procedure WriteFX(V: Integer);
    procedure WriteX(V: Integer);
    procedure Bump;
    procedure Show;
    property P: TV read Min;
  end;

  TC = class
    Min: TV;
    function ReadP: Integer;
    property P: TV read Min;
  end;

function TV.Twice: Integer;
begin
  Result := FX * 2;
end;

procedure AddTwo(var I: Integer);
begin
  I := I + 2;
end;

function TB.ReadX: Integer;
begin
  Result := Min.X;
end;

procedure TB.WriteFX(V: Integer);
begin
  Min.FX := V;
end;

procedure TB.WriteX(V: Integer);
begin
  Min.X := V;
end;

procedure TB.Bump;
begin
  AddTwo(Min.FX);
  Box.V.FX := Box.V.FX + 10;
  Shared.FX := Shared.FX + 100;
end;

procedure TB.Show;
begin
  WriteLn(Min.X, ' ', Min.Twice, ' ', P.FX, ' ', Box.V.X, ' ', Shared.X);
end;

function TC.ReadP: Integer;
begin
  Result := P.FX;
end;

var
  B: TB;
  C: TC;
begin
  B := Default(TB);
  B.WriteFX(3);
  WriteLn(B.ReadX);
  B.WriteX(5);
  WriteLn(B.Min.FX);
  B.Bump;
  B.Show;
  C := TC.Create;
  C.Min.FX := 9;
  WriteLn(C.ReadP);
  C.Free;
end.
