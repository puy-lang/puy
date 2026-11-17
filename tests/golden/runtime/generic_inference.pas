program GenericInference;

// Type argument inference for calls without type arguments (generics.md 5.2, 5.3, 5.4): the
// example of 5.3 (`T` from the parameter types of an anonymous method), a constant written
// without a type taking the type of a typed argument (`Second(N, 1)` with `N: Int64` gives an
// `Int64`), a function name without parentheses for `T` (a call: its result type is a strong
// candidate), the elements of `[…]`, a dynamic array, the parameters and result of a routine name
// and of a value of the same procedural type, an instantiation of the same definition, generic
// methods of a class and of an instantiated class, a call from a generic body, and the choice
// between generic and non-generic candidates (the non-generic one when the stages are the same).

type
  TBox<T> = class
    FItem: T;
    function Map<U>(const F: TFunc<T, U>): U;
  end;

  TUtil = class
    class function Pick<T>(const A, B: T; First: Boolean): T; static;
  end;

function TBox<T>.Map<U>(const F: TFunc<T, U>): U;
begin
  Result := F(FItem);
end;

class function TUtil.Pick<T>(const A, B: T; First: Boolean): T;
begin
  if First then Result := A else Result := B;
end;

function Max<T>(const A, B: T; const Less: TFunc<T, T, Boolean>): T;
begin
  if Less(A, B) then Result := B else Result := A;
end;

function Second<T>(const A, B: T): T;
begin
  Result := B;
end;

function Count<T>(const A: array of T): Integer;
begin
  Result := Length(A);
end;

function Last<T>(const A: TArray<T>): T;
begin
  Result := A[High(A)];
end;

function Apply<T, U>(const F: TFunc<T, U>; const X: T): U;
begin
  Result := F(X);
end;

function Unbox<T>(const B: TBox<T>): T;
begin
  Result := B.FItem;
end;

function Wrap<T>(const X: T): T;
begin
  // `Second` is inferred with the type parameter `T` of this body
  Result := Second(X, X);
end;

function Describe(const X: Integer): string; overload;
begin
  Result := 'Integer (not generic)';
end;

function Describe<T>(const X: T): string; overload;
begin
  Result := 'generic';
end;

function Twice(X: Integer): Integer;
begin
  Result := X * 2;
end;

function Huge: Int64;
begin
  Result := 5000000000;
end;

function Less(A, B: Integer): Boolean;
begin
  Result := A < B;
end;

var
  N: Int64;
  F: TFunc<Integer, string>;
  B: TBox<string>;
  Words: TArray<string>;
begin
  // generics.md 5.3
  var I := Max(3, 5, function(A, B: Integer): Boolean begin Result := A < B end);
  var S := Max<string>('a', 'b', function(A, B: string): Boolean begin Result := A < B end);
  WriteLn(I, ' ', S);

  // `T = Int64` from `N`; `1` is passed as an `Int64`
  N := 7;
  var Wide := Second(N, 1);
  Wide := 5000000000;
  WriteLn(Wide);

  // `Huge` is called: `T = Int64` from its result, not `Integer` from `1`
  WriteLn(Second(Huge, 1) + Huge);

  // The elements of `[…]` and a dynamic array
  WriteLn(Count([1, 2, 3]));
  WriteLn(Count(['a', 'b']));
  Words := ['p', 'q', 'r'];
  WriteLn(Last(Words));

  // A routine name, a value of the same procedural type, an instantiation
  WriteLn(Apply(Twice, 4));
  F := function(X: Integer): string begin Result := '#' + X.ToString end;
  WriteLn(Apply(F, 5));
  B := TBox<string>.Create;
  B.FItem := 'boxed';
  WriteLn(Unbox(B));

  // Generic methods
  WriteLn(TUtil.Pick(1.5, 2.5, False));
  WriteLn(B.Map(function(Text: string): Integer begin Result := Length(Text) end));
  WriteLn(Max(2, 9, Less));
  B.Free;

  // A call in a generic body
  WriteLn(Wrap('w'), ' ', Wrap(42));

  // The same heading: the declaration written with the types; a better stage: the generic one
  WriteLn(Describe(1));
  WriteLn(Describe(N));
  WriteLn(Describe('c'));
end.
