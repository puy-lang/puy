program KeyEqualsUserTypes;

// KeyEquals / KeyHash on the types whose equality the user decides (equality.md 2.5, 2.6, 3.1 to
// 3.3): a record declaring `Equal` and `GetHashCode` uses them; a reference counted class
// declaring `Equal` is compared through its generated `Equals` and its `GetHashCode`, also as an
// interface value; a class overriding `Equals` / `GetHashCode` is compared through them. A
// destroyed instance is not touched: it equals only itself, and its hash is its handle's.

type
  TAngle = record
    Deg: Integer;
    class operator Equal(const A, B: TAngle): Boolean;
    function GetHashCode: Int64;
  end;

  IValue = interface
    function GetV: Integer;
  end;

  TNum = class(TRefObject, IValue)
    V: Integer;
    constructor Create(AV: Integer);
    function GetV: Integer;
    class operator Equal(const A, B: TNum): Boolean;
    function GetHashCode: Int64; override;
  end;

  TKey = class
    Id: Integer;
    constructor Create(AId: Integer);
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

class operator TAngle.Equal(const A, B: TAngle): Boolean;
begin
  Result := (A.Deg - B.Deg) mod 360 = 0;
end;

function TAngle.GetHashCode: Int64;
begin
  Result := KeyHash(((Deg mod 360) + 360) mod 360);
end;

constructor TNum.Create(AV: Integer);
begin
  V := AV;
end;

function TNum.GetV: Integer;
begin
  Result := V;
end;

class operator TNum.Equal(const A, B: TNum): Boolean;
begin
  Result := A.V = B.V;
end;

function TNum.GetHashCode: Int64;
begin
  Result := KeyHash(V);
end;

constructor TKey.Create(AId: Integer);
begin
  Id := AId;
end;

function TKey.Equals(Obj: TObject): Boolean;
begin
  WriteLn('  TKey.Equals');
  Result := (Obj is TKey) and (TKey(Obj).Id = Id);
end;

function TKey.GetHashCode: Int64;
begin
  WriteLn('  TKey.GetHashCode');
  Result := KeyHash(Id);
end;

var
  A1, A2: TAngle;
  N1, N2: TNum;
  I1, I2: IValue;
  K1, K2, K3: TKey;
begin
  A1.Deg := 30;
  A2.Deg := 390;
  WriteLn('record Equal: ', KeyEquals(A1, A2), ' ', KeyHash(A1) = KeyHash(A2));
  A2.Deg := 31;
  WriteLn('record Equal, different: ', KeyEquals(A1, A2));
  N1 := TNum.Create(5);
  N2 := TNum.Create(5);
  WriteLn('reference counted Equal: ', KeyEquals(N1, N2), ' ', KeyHash(N1) = KeyHash(N2));
  WriteLn('as TObject: ', KeyEquals<TObject>(N1, N2));
  I1 := N1;
  I2 := N2;
  WriteLn('as interface: ', KeyEquals(I1, I2), ' ', KeyHash(I1) = KeyHash(I2));
  N2.V := 6;
  WriteLn('reference counted Equal, different: ', KeyEquals(N1, N2));
  K1 := TKey.Create(1);
  K2 := TKey.Create(1);
  K3 := K1;
  WriteLn('overridden Equals:');
  WriteLn(KeyEquals(K1, K2));
  WriteLn('the same instance, without calling Equals:');
  WriteLn(KeyEquals(K1, K3));
  WriteLn('overridden GetHashCode:');
  WriteLn(KeyHash(K1) = KeyHash(K2));
  // `K1` and `K3` are the same destroyed instance; `K2` is alive
  K1.Free;
  WriteLn('destroyed: ', KeyEquals(K1, K3), ' ', KeyEquals(K1, K2), ' ', KeyEquals(K2, K1), ' ',
    KeyHash(K1) = KeyHash(K3));
  WriteLn('destroyed and nil: ', KeyEquals<TKey>(K1, nil));
  K2.Free;
end.
