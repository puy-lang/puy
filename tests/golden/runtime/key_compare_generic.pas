program KeyCompareGeneric;

// KeyCompare<T> in a generic body (collections.md 10.2): it puts no requirement on `T`, so the
// body is instantiated with any type. A type with a default order is compared by it; one without
// raises EInvalidOperation when KeyCompare is called (after its arguments are evaluated), not
// when the body is instantiated or entered.

uses System.SysUtils;

type
  TPlain = record
    X: Integer;
  end;

  TVersion = record
    Major: Integer;
    class operator LessThan(const A, B: TVersion): Boolean;
  end;

  TNumbers = array of Integer;

class operator TVersion.LessThan(const A, B: TVersion): Boolean;
begin
  Result := A.Major < B.Major;
end;

function Larger<T>(const A, B: T): T;
begin
  if KeyCompare(A, B) >= 0 then
    Result := A
  else
    Result := B;
end;

function Compare<T>(const A, B: T): Integer;
begin
  WriteLn('comparing');
  Result := KeyCompare<T>(A, B);
  WriteLn('not reached');
end;

function Arg<T>(const X: T): T;
begin
  WriteLn('argument evaluated');
  Result := X;
end;

procedure CompareArgs<T>(const A, B: T);
begin
  KeyCompare(Arg<T>(A), Arg<T>(B));
end;

procedure Fails<T>(const A, B: T);
begin
  try
    Compare<T>(A, B);
  except
    on E: EInvalidOperation do
      WriteLn(E.ClassName, ': ', E.Message);
  end;
end;

var
  V1, V2: TVersion;
  P: TPlain;
  O: TObject;
  N: TNumbers;
begin
  WriteLn(Larger(3, 7), ' ', Larger('pear', 'apple'), ' ', Larger(2.5, -1.0));
  V1.Major := 2;
  V2.Major := 5;
  WriteLn(Larger(V1, V2).Major);
  P.X := 1;
  O := TObject.Create;
  Fails<TPlain>(P, P);
  Fails<TObject>(O, nil);
  Fails<TNumbers>(N, N);
  Fails<TObject>(nil, nil);
  try
    CompareArgs<TObject>(O, O);
  except
    on E: EInvalidOperation do
      WriteLn(E.Message);
  end;
  O.Free;
end.
