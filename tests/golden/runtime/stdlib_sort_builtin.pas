program StdlibSortBuiltin;

// Sort without a comparer orders the items of a type whose default order is a built-in comparison
// (an unsigned UInt64, Single, Currency, UTF8String, an enumeration) in the VM, in the same order
// as KeyCompare (collections.md 10.2, 10.3; bytecode.md 2.22). A record with its own LessThan,
// a comparer given to Sort and a comparer given to Create still go through the Pascal sort: the
// counts show that the Pascal code ran for each comparison.

uses System.SysUtils, System.Generics.Defaults, System.Generics.Collections;

type
  TColor = (Red, Green, Blue);
  TKey = record
    Value: Integer;
    class operator LessThan(const A, B: TKey): Boolean;
  end;

var
  Calls: Integer;

class operator TKey.LessThan(const A, B: TKey): Boolean;
begin
  Inc(Calls);
  Result := A.Value < B.Value;
end;

function Key(V: Integer): TKey;
begin
  Result := Default(TKey);
  Result.Value := V;
end;

function Down(const A, B: Integer): Integer;
begin
  Inc(Calls);
  Result := B - A;
end;

var
  U: TList<UInt64>;
  S: TArray<Single>;
  C: TList<Currency>;
  T: TList<UTF8String>;
  E: TArray<TColor>;
  K: TList<TKey>;
  I: TList<Integer>;
  X: UInt64;
  Y: Single;
  Z: Currency;
  W: UTF8String;
  Col: TColor;
  N: Integer;
begin
  U := TList<UInt64>.Create;
  U.AddRange([High(UInt64), 5, UInt64(High(Int64)) + 1, 0]);
  U.Sort;
  for X in U do
    Write(X, ' ');
  WriteLn;
  U.Free;

  S := [2.5, -1.0, 0.0 / 0.0, -0.5];
  TArray.Sort<Single>(S);
  for Y in S do
    Write(FloatToStr(Y), ' ');
  WriteLn;

  C := TList<Currency>.Create;
  C.AddRange([12.5, -0.0001, 3, -7.25]);
  C.Sort;
  for Z in C do
    Write(Z:0:4, ' ');
  WriteLn;
  C.Free;

  T := TList<UTF8String>.Create;
  T.AddRange([UTF8String('ü'), UTF8String('b'), UTF8String(''), UTF8String('B'), UTF8String('a')]);
  T.Sort;
  for W in T do
    Write('[', string(W), ']');
  WriteLn;
  T.Free;

  E := [TColor.Blue, TColor.Red, TColor.Blue, TColor.Green];
  TArray.Sort<TColor>(E);
  for Col in E do
    Write(Ord(Col));
  WriteLn;

  // The Pascal sort: LessThan for each comparison
  K := TList<TKey>.Create;
  for N := 1 to 20 do
    K.Add(Key((N * 7) mod 11));
  Calls := 0;
  K.Sort;
  for N := 0 to K.Count - 1 do
    Write(K[N].Value, ' ');
  WriteLn;
  WriteLn('LessThan called: ', Calls > 0);
  K.Free;

  // A comparer given to Sort, and one given to Create
  I := TList<Integer>.Create;
  I.AddRange([3, 1, 2]);
  Calls := 0;
  I.Sort(TComparer<Integer>.Construct(Down));
  WriteLn(I[0], I[1], I[2], ' comparer called: ', Calls > 0);
  I.Free;
  I := TList<Integer>.Create(TComparer<Integer>.Construct(Down));
  I.AddRange([1, 3, 2]);
  Calls := 0;
  I.Sort;
  WriteLn(I[0], I[1], I[2], ' comparer called: ', Calls > 0);
  I.Free;
end.
