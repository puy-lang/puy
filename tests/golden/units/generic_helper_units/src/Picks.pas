unit Picks;

// A helper with generic methods declared in the interface section (generics.md 1.2, 2.1): the
// bodies in the implementation section write the type parameters again, without the constraints.

interface

type
  TCounter = class
    Count: Integer;
  end;

  TCounterHelper = class helper for TCounter
    function Pick<T>(const A, B: T): T;
    function Same<T: class>(const A, B: T): Boolean;
    class function Repeated<T>(const X: T; N: Integer): TArray<T>; static;
  end;

implementation

function TCounterHelper.Pick<T>(const A, B: T): T;
begin
  Count := Count + 1;
  if Odd(Count) then
    Result := A
  else
    Result := B;
end;

function TCounterHelper.Same<T>(const A, B: T): Boolean;
begin
  Result := A = B;
end;

class function TCounterHelper.Repeated<T>(const X: T; N: Integer): TArray<T>;
var
  I: Integer;
begin
  SetLength(Result, N);
  for I := 0 to N - 1 do
    Result[I] := X;
end;

end.
