program GenericRoutines;

// Generic routines and methods (generics.md 1.2, 2.1, 4.1, 5.1, 5.4): routines of the program
// (Puy only; Delphi has generic methods only), `class` and `static` methods, instance methods of a
// class and of a record, a generic method of a generic class called from outside and from its own
// body, a generic routine calling another with its own type parameter, a recursive call, a nested
// routine and an anonymous method in a generic body, a call without parentheses, values of
// generic routines and methods, and overloads chosen among the candidates with the same number of
// type parameters, also when they are declared `forward` or in a class and their bodies come in
// another order.

type
  TUtil = class
    class function Max<T>(const A, B: T; BFirst: Boolean): T; static;
    class function Name<T>: string;
    function Keep<T>(const A: T): T;
    class function F<T>(const A: T): T; overload; static;
    class function F<T>(const A, B: T): T; overload; static;
  end;

  TPair<K, V> = record
    Key: K;
    Value: V;
    function Swap<X>(const Extra: X): TPair<V, K>;
    class function Make<A>(const Key: A; const Value: V): TPair<A, V>; static;
  end;

  TList<T> = class
    Items: TArray<T>;
    procedure Add(const X: T);
    function Fill<U>(const F: U): TArray<U>;
    function Twice<U>(const F: U): Integer;
    function Depth<U>(N: Integer): Integer;
  end;

  TIntFn = function(const A, B: Integer; BFirst: Boolean): Integer;
  TStrFn = function(const A: string): string;
  TKeep = function(const A: string): string of object;
  TGet = reference to function: string;

class function TUtil.Max<T>(const A, B: T; BFirst: Boolean): T;
begin
  if BFirst then
    Result := B
  else
    Result := A;
end;

class function TUtil.Name<T>: string;
begin
  Result := ClassName;
end;

function TUtil.Keep<T>(const A: T): T;
begin
  Result := A;
end;

class function TUtil.F<T>(const A, B: T): T;
begin
  Result := B;
end;

class function TUtil.F<T>(const A: T): T;
begin
  Result := A;
end;

function Fwd<T>(const A: T): T; overload; forward;
function Fwd<T>(const A, B: T): T; overload; forward;

function Fwd<T>(const A, B: T): T;
begin
  Result := B;
end;

function Fwd<T>(const A: T): T;
begin
  Result := A;
end;

function Fwd<T, U>(const A: T; const B: U): T; overload;
begin
  Result := A;
end;

function TPair<K, V>.Swap<X>(const Extra: X): TPair<V, K>;
begin
  Result := Default(TPair<V, K>);
  Result.Key := Value;
  Result.Value := Key;
end;

class function TPair<K, V>.Make<A>(const Key: A; const Value: V): TPair<A, V>;
begin
  Result := Default(TPair<A, V>);
  Result.Key := Key;
  Result.Value := Value;
end;

procedure TList<T>.Add(const X: T);
begin
  SetLength(Items, Length(Items) + 1);
  Items[High(Items)] := X;
end;

function TList<T>.Fill<U>(const F: U): TArray<U>;
begin
  SetLength(Result, Length(Items));
  for var I := 0 to High(Items) do
    Result[I] := F;
end;

function TList<T>.Twice<U>(const F: U): Integer;
begin
  Result := Length(Fill<U>(F)) * 2;
end;

function TList<T>.Depth<U>(N: Integer): Integer;
begin
  if N = 0 then
    Exit(0);
  Result := 1 + Depth<U>(N - 1);
end;

function Pick<T>(const A, B: T; First: Boolean): T;
var
  Tmp: T;

  function Inner: T;
  begin
    Result := A;
  end;

begin
  if First then
    Tmp := Inner
  else
    Tmp := B;
  Result := Tmp;
end;

function Both<T>(const A: T): TArray<T>;
begin
  Result := [Pick<T>(A, A, True), A];
end;

function Count<T>: Integer;
begin
  Result := 3;
end;

function Depth<T>(N: Integer): Integer;
begin
  if N = 0 then
    Exit(0);
  Result := 1 + Depth<T>(N - 1);
end;

function Later<T>(const X: T): TGet;
var
  Copy: T;
begin
  Copy := X;
  Result := function: string
    begin
      Copy := Pick<T>(Copy, Default(T), False);
      Result := 'later';
    end;
end;

function Plus(const A, B: Integer): Integer; overload;
begin
  Result := A + B;
end;

function Plus<T>(const A: T): T; overload;
begin
  Result := A;
end;

function Plus<T>(const A, B: T; C: Integer): T; overload;
begin
  Result := B;
end;

function Plus<T, U>(const A: T; const B: U): U; overload;
begin
  Result := B;
end;

var
  U: TUtil;
  P: TPair<string, Integer>;
  Q: TPair<Integer, string>;
  L: TList<Integer>;
  S: TArray<string>;
  F: TIntFn;
  G: TStrFn;
  K: TKeep;
  Get: TGet;
begin
  WriteLn(Pick<Integer>(1, 2, False), ' ', Pick<string>('a', 'b', True));
  S := Both<string>('t');
  WriteLn(Length(S), ' ', S[1]);
  WriteLn(TUtil.Max<Double>(1.5, 2.5, True), ' ', TUtil.Name<Integer>);
  U := TUtil.Create;
  WriteLn(U.Keep<string>('kept'));
  P.Key := 'a';
  P.Value := 1;
  Q := P.Swap<Double>(1.5);
  WriteLn(Q.Key, ' ', Q.Value);
  WriteLn(TPair<string, Integer>.Make<Char>('c', 2).Key);
  L := TList<Integer>.Create;
  L.Add(10);
  L.Add(20);
  S := L.Fill<string>('s');
  WriteLn(S[0], S[1], ' ', L.Twice<Char>('z'), ' ', L.Depth<Byte>(5));
  WriteLn(Count<string> * 2, ' ', Count<Integer>() + 1, ' ', Depth<Byte>(4));
  Get := Later<Double>(1.5);
  WriteLn(Get());
  WriteLn(Plus(1, 2), ' ', Plus<Integer>(5), ' ', Plus<string>('a', 'b', 0), ' ',
    Plus<Integer, string>(1, 'u'));
  F := TUtil.Max<Integer>;
  WriteLn(F(1, 9, False));
  G := Plus<string>;
  WriteLn(G('g'));
  K := U.Keep<string>;
  WriteLn(K('k'));
  WriteLn(TUtil.F<Integer>(1), ' ', TUtil.F<Integer>(1, 2), ' ', Fwd<string>('f'), ' ',
    Fwd<string>('f', 'g'), ' ', Fwd<Integer, string>(7, 'x'));
  L.Free;
  U.Free;
end.
