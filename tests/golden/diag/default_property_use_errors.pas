program DefaultPropertyUseErrors;

// Errors in uses of default array properties and `TIndexRange` (objects.md 12.1-12.4): indices
// no default property takes (E0735, with a note on `TIndexRange` for a range), indices two take
// equally well (E0734), a range written through the name (E0720), a bound that is not an integer
// (E0719), a read-only one written (E0707), one passed to `var` and `Inc` (E0706), a field of its
// value written (E0707), one that is not visible (E0743), `X[…]` on a record without one (E0720
// with a note), the wrong number of indices of the only one (E0705), and `Clamp` called with the
// wrong arguments (E0705, E0706, E0702), used as a value (E0725) and as a procedural value (E0752).

type
  TPoint = record
    X, Y: Integer;
  end;

  TBag = class(TRefObject)
    function GetI(I: Integer): Integer;
    function GetS(const S: string): Integer;
    property Items[I: Integer]: Integer read GetI; default;
    property Items[const S: string]: Integer read GetS; default;
  end;

  TWide = class(TRefObject)
    function GetA(I: Int64): Integer;
    function GetB(I: Cardinal): Integer;
    property Items[I: Int64]: Integer read GetA; default;
    property Items[I: Cardinal]: Integer read GetB; default;
  end;

  TPoints = class(TRefObject)
    function GetP(I: Integer): TPoint;
    property Points[I: Integer]: TPoint read GetP; default;
  end;

  THidden = class(TRefObject)
  strict private
    function GetH(I: Integer): Integer;
    property Items[I: Integer]: Integer read GetH; default;
  end;

function TBag.GetI(I: Integer): Integer;
begin
  Result := I;
end;

function TBag.GetS(const S: string): Integer;
begin
  Result := Length(S);
end;

function TWide.GetA(I: Int64): Integer;
begin
  Result := 1;
end;

function TWide.GetB(I: Cardinal): Integer;
begin
  Result := 2;
end;

function TPoints.GetP(I: Integer): TPoint;
begin
  Result := Default(TPoint);
  Result.X := I;
end;

function THidden.GetH(I: Integer): Integer;
begin
  Result := I;
end;

procedure Bump(var N: Integer);
begin
  Inc(N);
end;

var
  B: TBag;
  W: TWide;
  P: TPoints;
  H: THidden;
  Pt: TPoint;
  R: TIndexRange;
  F, L: Integer;
  Proc: procedure;
begin
  B := TBag.Create;
  W := TWide.Create;
  P := TPoints.Create;
  H := THidden.Create;
  WriteLn(B[1.5]);
  WriteLn(B[1..2]);
  WriteLn(B[1, 2]);
  WriteLn(W[Byte(1)]);
  WriteLn(B.Items[1..2]);
  WriteLn(B['a'..'b']);
  B[1] := 2;
  Bump(B[1]);
  Inc(B['a']);
  P[0].X := 1;
  WriteLn(H[1]);
  WriteLn(Pt[1]);
  WriteLn(P[1, 2].X);
  R.Clamp(0, 9, F);
  R.Clamp(0, 9, F, 1);
  R.Clamp(0, 'x', F, L);
  F := R.Clamp(0, 9, F, L);
  Proc := R.Clamp;
end.
