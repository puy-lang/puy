program GenericRuleEdges;

// The rules of generics.md left out by the other cases, run: type arguments named by an alias or
// a named dynamic array give the same instantiation as the type itself, and an inferred anonymous
// type is named in `ClassName` (4.1); a nested enumeration is one type across instantiations
// (2.5); a generic method of a class nested in a class (1.2); `var` / `out` of `T` and
// `Insert` / `Delete` / `Low` / `High` on `TArray<T>` in a generic body (3.3, 6.1); `for var` of
// an instantiation, `as`, a cast in an expression and `>=` split before `= nil` (1.3, 1.4);
// `TProc` / `TFunc` with four arguments (6.2); a `sealed` class as a constraint (3.2); and a
// declared `NotRefCounted` hiding the built-in constraint (3.6).

type
  TInts = array of Integer;
  TAlias = Integer;

  TBox<T> = class
    V: T;
    procedure Show;
  end;

  TOuter<T> = class
  type
    TKind = (kA, kB);
  public
    class var K: TKind;
  end;

  THolder = class
  type
    TInner = class
      function Map<U>(const X: U): U;
    end;
  end;

  TSealed = class sealed
  end;
  TOnlySealed<T: TSealed> = class
  end;

  NotRefCounted = class
  end;
  TOther = class(NotRefCounted)
  end;
  TMine<T: NotRefCounted> = class
  end;

procedure TBox<T>.Show;
begin
  WriteLn(ClassName);
end;

function THolder.TInner.Map<U>(const X: U): U;
begin
  Result := X;
end;

function Wrap<T>(const X: T): TBox<T>;
begin
  Result := TBox<T>.Create;
  Result.V := X;
end;

procedure Swap<T>(var A, B: T);
var
  C: T;
begin
  C := A;
  A := B;
  B := C;
end;

procedure Give<T>(const V: T; out R: T);
begin
  R := V;
end;

function Ops<T>(const A: TArray<T>; const X: T): TArray<T>;
begin
  Result := Copy(A, 0, Length(A));
  Insert(X, Result, 0);
  Delete(Result, High(Result), 1);
  WriteLn(Low(Result), ' ', High(Result));
end;

var
  A: array of Integer;
  I, J: Integer;
  S, U: string;
  L: TArray<TBox<Integer>>;
  O: TObject;
  D: TBox<Integer>= nil;
  Inner: THolder.TInner;
  P4: TProc<Integer, Integer, Integer, string>;
  F4: TFunc<Integer, Integer, Integer, Integer, string>;
  OS: TOnlySealed<TSealed>;
  Mine: TMine<TOther>;
begin
  A := [1, 2];
  var B := Wrap(A);
  var C: TBox<TInts> := B;
  var E: TBox<TAlias> := TBox<Integer>.Create;
  B.Show;
  WriteLn(Length(C.V));
  E.V := 3;
  WriteLn(E.V);
  B.Free;
  E.Free;

  TOuter<string>.K := TOuter<Integer>.TKind.kB;
  TOuter<Integer>.K := TOuter<string>.K;
  WriteLn(Ord(TOuter<Integer>.K), Ord(TOuter<string>.K));

  Inner := THolder.TInner.Create;
  WriteLn(Inner.Map<Integer>(4), Inner.Map('s'));
  Inner.Free;

  I := 1;
  J := 2;
  Swap(I, J);
  WriteLn(I, J);
  S := 'a';
  U := 'b';
  Swap<string>(S, U);
  WriteLn(S, U);
  Give('x', S);
  WriteLn(S);
  for var X in Ops<string>(['p', 'q', 'r'], 'z') do
    Write(X);
  WriteLn;

  SetLength(L, 1);
  L[0] := TBox<Integer>.Create;
  L[0].V := 7;
  for var Bx: TBox<Integer> in L do
    WriteLn(Bx.V);
  O := L[0];
  WriteLn((O as TBox<Integer>).V, ' ', TBox<Integer>(O).V, ' ', D = nil);
  L[0].Free;

  P4 := procedure(P, Q, R: Integer; Z: string)
    begin
      WriteLn(P + Q + R, Z);
    end;
  P4(1, 2, 3, 'x');
  F4 := function(P, Q, R, Z: Integer): string
    begin
      Result := 'f' + Chr(Ord('0') + P + Q + R - Z);
    end;
  WriteLn(F4(1, 2, 3, 4));

  OS := TOnlySealed<TSealed>.Create;
  OS.Free;
  Mine := TMine<TOther>.Create;
  Mine.Free;
  WriteLn('done');
end.
