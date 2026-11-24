program DumpUnreachable;

// Only the functions an entry reaches go in the module (units.md 8.3). Dropped: `Unused`, the
// method `TShape.Unused` nobody calls, and the methods of `TUnusedShape`, a class no kept
// function names. Kept: `TSquare.Area` and `TShape.Destroy`, named by the virtual method table
// of `TSquare`, a class created; `TCircle.Area`, named by the table of `TCircle`, which is in the
// type section only through a class reference; `TCounter.Next`, named by its interface table;
// `Twice`, loaded as a value; `TCounter.Reset`, loaded as a method value; `Captured.<anon1>`, the
// function of a closure; `Outer.Inner`, called by `Outer`.

type
  ICounter = interface
    function Next: Integer;
  end;

  TShape = class
    function Area: Integer; virtual;
    function Unused: Integer;
    destructor Destroy; override;
  end;

  TSquare = class(TShape)
    Side: Integer;
    function Area: Integer; override;
  end;

  TCircle = class(TShape)
    function Area: Integer; override;
  end;

  TShapeClass = class of TShape;

  TUnusedShape = class(TShape)
    function Area: Integer; override;
  end;

  TCounter = class(TRefObject, ICounter)
    N: Integer;
    function Next: Integer;
    procedure Reset;
  end;

  TIntFunc = function(X: Integer): Integer;
  TNotify = procedure of object;

function TShape.Area: Integer;
begin
  Result := 0;
end;

function TShape.Unused: Integer;
begin
  Result := Area + 1;
end;

destructor TShape.Destroy;
begin
  WriteLn('destroy');
end;

function TSquare.Area: Integer;
begin
  Result := Side * Side;
end;

function TCircle.Area: Integer;
begin
  Result := 7;
end;

function TUnusedShape.Area: Integer;
begin
  Result := -1;
end;

function TCounter.Next: Integer;
begin
  N := N + 1;
  Result := N;
end;

procedure TCounter.Reset;
begin
  N := 0;
end;

function Captured(X: Integer): TFunc<Integer>;
begin
  Result := function: Integer
    begin
      Result := X + 10;
    end;
end;

procedure Unused;
begin
  WriteLn(TUnusedShape.ClassName);
end;

function Twice(X: Integer): Integer;
begin
  Result := X * 2;
end;

function Outer(X: Integer): Integer;

  function Inner: Integer;
  begin
    Result := X + 1;
  end;

begin
  Result := Inner;
end;

var
  S: TShape;
  C: ICounter;
  F: TIntFunc;
  K: TShapeClass;
  Counter: TCounter;
  R: TNotify;
begin
  S := TSquare.Create;
  TSquare(S).Side := 3;
  WriteLn(S.Area);
  S.Free;
  C := TCounter.Create;
  WriteLn(C.Next);
  F := Twice;
  WriteLn(F(Outer(1)));
  K := TCircle;
  S := K.Create;
  WriteLn(S.Area);
  S.Free;
  Counter := TCounter.Create;
  R := Counter.Reset;
  R;
  WriteLn(Counter.N);
  WriteLn(Captured(1)());
end.
