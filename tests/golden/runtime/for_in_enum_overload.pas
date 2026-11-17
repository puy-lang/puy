program ForInEnumOverload;

// An overloaded `GetEnumerator` (objects.md 13.4): `for..in` uses the version without
// parameters, and the other one can still be called by name.

type
  TCounter = class
    I, Last: Integer;
    function MoveNext: Boolean;
    function GetCurrent: Integer;
    property Current: Integer read GetCurrent;
  end;

  TRange = class
    function GetEnumerator(Last: Integer): TCounter; overload;
    function GetEnumerator: TCounter; overload;
  end;

function TCounter.MoveNext: Boolean;
begin
  Inc(I);
  Result := I <= Last;
end;

function TCounter.GetCurrent: Integer;
begin
  Result := I;
end;

function TRange.GetEnumerator(Last: Integer): TCounter;
begin
  Result := TCounter.Create;
  Result.Last := Last;
end;

function TRange.GetEnumerator: TCounter;
begin
  WriteLn('parameterless GetEnumerator');
  Result := GetEnumerator(3);
end;

var
  R: TRange;
  E: TCounter;
begin
  R := TRange.Create;
  for var X in R do
    WriteLn(X);
  E := R.GetEnumerator(2);
  while E.MoveNext do
    WriteLn('by name ', E.Current);
  E.Free;
  R.Free;
end.
