program GenericMethods;

// Generic methods that run the same under FPC and Delphi (generics.md 1.2, 1.4, 4.1): a
// `class` method and a `static` method of a class written with type arguments, an instance
// method of a class and of a record, a generic method calling another with its own type
// parameter, and a generic type used in a generic method. (Generic routines outside a class are
// Puy only, and FPC 3.2.2 does not take a generic method of a generic type: golden/runtime.)

type
  TBox<T> = class
    Value: T;
  end;

  TUtil = class
    class function Pick<T>(const A, B: T; First: Boolean): T;
    class function Both<T>(const A: T): TArray<T>; static;
    function Keep<T>(const A: T): T;
    function Boxed<T>(const A: T): TBox<T>;
  end;

  TCounter = record
    Count: Integer;
    function Fill<T>(const Value: T): TArray<T>;
  end;

class function TUtil.Pick<T>(const A, B: T; First: Boolean): T;
begin
  if First then
    Result := A
  else
    Result := B;
end;

class function TUtil.Both<T>(const A: T): TArray<T>;
begin
  SetLength(Result, 2);
  Result[0] := Pick<T>(A, A, True);
  Result[1] := A;
end;

function TUtil.Keep<T>(const A: T): T;
begin
  Result := A;
end;

function TUtil.Boxed<T>(const A: T): TBox<T>;
begin
  Result := TBox<T>.Create;
  Result.Value := A;
end;

function TCounter.Fill<T>(const Value: T): TArray<T>;
var
  I: Integer;
begin
  SetLength(Result, Count);
  for I := 0 to Count - 1 do
    Result[I] := Value;
end;

var
  U: TUtil;
  C: TCounter;
  S: TArray<string>;
  B: TBox<Integer>;
begin
  WriteLn(TUtil.Pick<Integer>(1, 2, False));
  WriteLn(TUtil.Pick<string>('a', 'b', True));
  S := TUtil.Both<string>('t');
  WriteLn(Length(S), ' ', S[1]);
  U := TUtil.Create;
  WriteLn(U.Keep<Double>(2.5):0:1);
  B := U.Boxed<Integer>(42);
  WriteLn(B.Value);
  B.Free;
  U.Free;
  C.Count := 3;
  S := C.Fill<string>('s');
  WriteLn(Length(S), ' ', S[2]);
  WriteLn(Length(C.Fill<Boolean>(True)));
end.
