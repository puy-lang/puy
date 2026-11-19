program GenericRoutineNestingLimit;

// A generic routine and a generic static method whose bodies ask for deeper instantiations of
// themselves without end: E0764 once each, with the path of the routine instantiations in the
// notes, as for a type (generics.md 4.4). A path where a generic routine and a method of a
// generic type ask for each other shows both kinds, from either end.

function F<T>(const A: T; N: Integer): Integer;
begin
  if N = 0 then
    Result := 0
  else
    Result := F<TArray<T>>(nil, N - 1);
end;

type
  TU = class
    class function G<T>(const A: T; N: Integer): Integer; static;
  end;

class function TU.G<T>(const A: T; N: Integer): Integer;
begin
  if N = 0 then
    Result := 0
  else
    Result := TU.G<TArray<T>>(nil, N - 1);
end;

type
  TBox<T> = class
    class function M(N: Integer): Integer; static;
  end;

  TDeep<T> = class
    procedure Go;
  end;

function H<T>(N: Integer): Integer;
begin
  if N = 0 then
    Result := 0
  else
    Result := TBox<T>.M(N - 1);
end;

class function TBox<T>.M(N: Integer): Integer;
begin
  Result := H<TArray<T>>(N);
end;

procedure K<T>;
var
  D: TDeep<TArray<T>>;
begin
  D := TDeep<TArray<T>>.Create;
  D.Go;
  D.Free;
end;

procedure TDeep<T>.Go;
begin
  K<T>;
end;

var
  D: TDeep<Integer>;
begin
  WriteLn(F<Integer>(1, 3));
  WriteLn(TU.G<Integer>(1, 3));
  WriteLn(H<Integer>(3));
  D := TDeep<Integer>.Create;
  D.Go;
  D.Free;
end.
