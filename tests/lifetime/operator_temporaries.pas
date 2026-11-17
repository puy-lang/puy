// puy-test: memory-limit=1m
program OperatorTemporaries;

// The results of the operators of a reference counted class are temporaries like a function's
// result (objects.md 11.5): the instances in the middle of `A + B + C`, the ones `Implicit`
// makes for an operand, and the result of a comparison's operands are let go, so the loop stays
// inside the 1 MB limit, every instance is destroyed, and nothing is left at the end.

type
  TBig = class(TRefObject)
    Data: array of Integer;
    constructor Create(N: Integer);
    destructor Destroy; override;
    class operator Add(const A, B: TBig): TBig;
    class operator Equal(const A, B: TBig): Boolean;
    function GetHashCode: Int64; override;
    class operator NotEqual(const A, B: TBig): Boolean;
    class operator Implicit(N: Integer): TBig;
  end;

var
  Made, Destroyed: Integer;

constructor TBig.Create(N: Integer);
begin
  SetLength(Data, 1000);
  Data[0] := N;
  Inc(Made);
end;

destructor TBig.Destroy;
begin
  Inc(Destroyed);
  inherited;
end;

class operator TBig.Add(const A, B: TBig): TBig;
begin
  Result := TBig.Create(A.Data[0] + B.Data[0]);
end;

class operator TBig.Equal(const A, B: TBig): Boolean;
begin
  Result := A.Data[0] = B.Data[0];
end;

function TBig.GetHashCode: Int64;
begin
  Result := Data[0];
end;

class operator TBig.NotEqual(const A, B: TBig): Boolean;
begin
  Result := A.Data[0] <> B.Data[0];
end;

class operator TBig.Implicit(N: Integer): TBig;
begin
  Result := TBig.Create(N);
end;

procedure Run;
var
  A, B, Sum: TBig;
  I, Hits: Integer;
begin
  A := TBig.Create(1);
  B := TBig.Create(2);
  Hits := 0;
  Sum := nil;
  for I := 1 to 500 do
  begin
    Sum := A + B + A + I;
    if Sum + 1 = A + B + I + 2 then
      Inc(Hits);
  end;
  WriteLn('sum ', Sum.Data[0], ', hits ', Hits);
end;

begin
  Run;
  WriteLn('made ', Made, ', destroyed ', Destroyed);
end.
