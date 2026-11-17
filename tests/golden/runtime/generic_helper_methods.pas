program GenericHelperMethods;

// Generic methods of helpers (generics.md 1.2, 2.1, 5.1): instance, `class` and `static` methods
// of a class helper, of a record helper for a record, for `Integer` and for `TArray<string>`, and
// of a class helper for an instance of a generic class; called with the type arguments written
// and inferred, in the form `H.M(X, …)` (objects.md 10.5), from the body of a child helper
// unqualified and through `Self` (10.6), with a `class var` of the helper counting the calls; and
// a helper nested in a class, whose body's heading names it through the class (2.5).

type
  TBox = class
    Value: Integer;
  end;

  TBoxHelper = class helper for TBox
    function Pick<T>(const A, B: T): T;
    class function Make<T>(const X: T): TArray<T>;
    class function Twice<T>(const X: T): TArray<T>; static;
    class var Calls: Integer;
  end;

  TBoxChildHelper = class helper(TBoxHelper) for TBox
    function PickBoth<T>(const A, B: T): TArray<T>;
  end;

  TPoint = record
    X, Y: Integer;
  end;

  TPointHelper = record helper for TPoint
    function Tag<T>(const V: T; const Sep: string): string;
    class function Pair<T>(const A, B: T): TArray<T>; static;
  end;

  TIntHelper = record helper for Integer
    function Choose<T>(const A, B: T): T;
  end;

  TStringsHelper = record helper for TArray<string>
    function Map<U>(const F: TFunc<string, U>): TArray<U>;
  end;

  TBag<T> = class
    Items: TArray<T>;
  end;

  TIntBagHelper = class helper for TBag<Integer>
    function Convert<U>(const F: TFunc<Integer, U>): TArray<U>;
  end;

  TOuter = class
  type
    TNestedHelper = class helper for TBox
      function First<T>(const A, B: T): T;
    end;
  end;

function TBoxHelper.Pick<T>(const A, B: T): T;
begin
  Inc(Calls);
  if Value > 0 then
    Result := A
  else
    Result := B;
end;

class function TBoxHelper.Make<T>(const X: T): TArray<T>;
begin
  Result := [X, X, X];
end;

class function TBoxHelper.Twice<T>(const X: T): TArray<T>;
begin
  Result := [X, X];
end;

function TBoxChildHelper.PickBoth<T>(const A, B: T): TArray<T>;
begin
  Result := [Pick(A, B), Self.Pick<T>(B, A)];
end;

function TPointHelper.Tag<T>(const V: T; const Sep: string): string;
begin
  Result := X.ToString + Sep + Y.ToString;
end;

class function TPointHelper.Pair<T>(const A, B: T): TArray<T>;
begin
  Result := [A, B];
end;

function TIntHelper.Choose<T>(const A, B: T): T;
begin
  if Self > 0 then
    Result := A
  else
    Result := B;
end;

function TStringsHelper.Map<U>(const F: TFunc<string, U>): TArray<U>;
var
  I: Integer;
begin
  SetLength(Result, Length(Self));
  for I := 0 to High(Self) do
    Result[I] := F(Self[I]);
end;

function TIntBagHelper.Convert<U>(const F: TFunc<Integer, U>): TArray<U>;
var
  I: Integer;
begin
  SetLength(Result, Length(Items));
  for I := 0 to High(Items) do
    Result[I] := F(Items[I]);
end;

function TOuter.TNestedHelper.First<T>(const A, B: T): T;
begin
  Result := A;
end;

var
  B: TBox;
  P: TPoint;
  N: Integer;
  R: TArray<Integer>;
  S: TArray<string>;
  Bag: TBag<Integer>;
  X: Integer;
  Y: string;
begin
  B := TBox.Create;
  B.Value := 1;
  WriteLn(B.Pick('a', 'b'));
  WriteLn(B.Pick<Integer>(1, 2));
  B.Value := 0;
  WriteLn(B.Pick(1.5, 2.5));
  WriteLn(TBoxHelper.Pick(B, 10, 20));
  WriteLn(TBoxHelper.Pick<string>(B, 'x', 'y'));
  WriteLn(Length(TBox.Make<string>('x')), ' ', Length(B.Make(3)), ' ', Length(TBox.Twice(True)));
  B.Value := 1;
  R := B.PickBoth(1, 2);
  WriteLn(R[0], ' ', R[1]);
  WriteLn(TBoxChildHelper.Calls);
  WriteLn(TOuter.TNestedHelper.First(B, 1, 2), ' ', TOuter.TNestedHelper.First<string>(B, 'a', 'b'));
  B.Free;

  P.X := 3;
  P.Y := 4;
  WriteLn(P.Tag(1, ','), ' ', P.Tag<Boolean>(True, ';'));
  WriteLn(Length(TPoint.Pair('a', 'b')));
  N := -1;
  WriteLn(N.Choose('pos', 'neg'), ' ', 5.Choose<Char>('p', 'n'));

  S := ['a', 'bb', 'ccc'];
  for X in S.Map<Integer>(function(X: string): Integer begin Result := Length(X) end) do
    Write(X, ' ');
  WriteLn;

  Bag := TBag<Integer>.Create;
  Bag.Items := [1, 2, 3];
  for Y in Bag.Convert(function(X: Integer): string begin Result := X.ToString + '!' end) do
    Write(Y, ' ');
  WriteLn;
  Bag.Free;
end.
