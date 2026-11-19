program DumpLocalTypeMethodNames;

// A method of a local type is named after the routine declaring the type (semantics.md 4.9):
// `A.TLoc.S` and `B.TLoc.S` with no overload number, the generated `B.TVal.Equals` and
// `C.TVal.Equals` of `[ValueEquality]` (equality.md 3.4), `A.Inner.TIn.S` in a nested routine,
// and the shared function of a generic body's local type `TBox<T>.Get.TLoc.S` (generics.md 2.6,
// 7).

type
  TBox<T> = class
    function Get(const X: T): Integer;
  end;

procedure A;
type
  TLoc = record
    N: Integer;
    procedure S;
  end;
procedure TLoc.S;
begin
  WriteLn('A ', N);
end;

  procedure Inner;
  type
    TIn = record
      M: Integer;
      procedure S;
    end;
  procedure TIn.S;
  begin
    WriteLn('A.Inner ', M);
  end;
  var
    L: TIn = Default(TIn);
  begin
    L.M := 3;
    L.S;
  end;

var
  L: TLoc = Default(TLoc);
begin
  L.N := 1;
  L.S;
  Inner;
end;

procedure B;
type
  TLoc = record
    N: Integer;
    procedure S;
  end;
  [ValueEquality]
  TVal = class
    N: Integer;
  end;
procedure TLoc.S;
begin
  WriteLn('B ', N);
end;
var
  L: TLoc = Default(TLoc);
  X, Y: TVal;
begin
  L.N := 2;
  L.S;
  X := TVal.Create;
  Y := TVal.Create;
  WriteLn(X.Equals(Y));
  X.Free;
  Y.Free;
end;

procedure C;
type
  [ValueEquality]
  TVal = class
    N: Integer;
  end;
var
  X, Y: TVal;
begin
  X := TVal.Create;
  Y := TVal.Create;
  Y.N := 1;
  WriteLn(X.Equals(Y));
  X.Free;
  Y.Free;
end;

function TBox<T>.Get(const X: T): Integer;
type
  TLoc = record
    V: T;
    function S: Integer;
  end;
function TLoc.S: Integer;
begin
  Result := 4;
end;
var
  L: TLoc = Default(TLoc);
begin
  L.V := X;
  Result := L.S;
end;

var
  Box: TBox<Integer>;
  Box2: TBox<Double>;
begin
  A;
  B;
  C;
  Box := TBox<Integer>.Create;
  Box2 := TBox<Double>.Create;
  WriteLn(Box.Get(5) + Box2.Get(0.5));
  Box.Free;
  Box2.Free;
end.
