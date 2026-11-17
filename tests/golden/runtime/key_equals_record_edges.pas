program KeyEqualsRecordEdges;

// KeyEquals / KeyHash on the edges of the type rules (equality.md 2.1, 2.5 to 2.7, 4): a record
// without fields equals any value of its type and hashes like any other such record; methods and
// `class var` do not take part; an `Equal` with another type's operand is not the record's own,
// so the fields are compared without calling it; a record field whose record declares `Equal` and
// `GetHashCode` is compared through them; `nil` class references are equal; `nil` of every
// reference kind hashes as `HashInt(0)`; and `KeyHash('abc')` infers `string`.

type
  TEmpty = record
  end;
  TEmpty2 = record
  end;

  TWithStuff = record
    X: Integer;
    class var Count: Integer;
    function Twice: Integer;
  end;

  TVec = record
    X: Double;
    class operator Equal(const A: TVec; B: Double): Boolean;
  end;

  TAngle = record
    Deg: Integer;
    class operator Equal(const A, B: TAngle): Boolean;
    function GetHashCode: Int64;
  end;

  THolder = record
    A: TAngle;
    N: Integer;
  end;

  TFoo = class
  end;
  TFooClass = class of TFoo;
  TProcP = procedure;
  TNotify = procedure of object;
  TAction = reference to procedure;

function TWithStuff.Twice: Integer;
begin
  Result := X * 2;
end;

class operator TVec.Equal(const A: TVec; B: Double): Boolean;
begin
  WriteLn('TVec.Equal called');
  Result := A.X = B;
end;

class operator TAngle.Equal(const A, B: TAngle): Boolean;
begin
  Result := (A.Deg - B.Deg) mod 360 = 0;
end;

function TAngle.GetHashCode: Int64;
begin
  Result := KeyHash(((Deg mod 360) + 360) mod 360);
end;

var
  E1, E2: TEmpty;
  E3: TEmpty2;
  W1, W2: TWithStuff;
  V1, V2: TVec;
  H1, H2: THolder;
  K: TFooClass;
  Z: Int64;
begin
  WriteLn('empty: ', KeyEquals(E1, E2), ' ', KeyHash(E1) = KeyHash(E2), ' ',
    KeyHash(E1) = KeyHash(E3));
  W1.X := 1;
  W2.X := 1;
  TWithStuff.Count := 5;
  WriteLn('fields only: ', KeyEquals(W1, W2), ' ', W1.Twice);
  V1.X := 1.5;
  V2.X := 1.5;
  WriteLn('other-typed Equal: ', KeyEquals(V1, V2), ' ', KeyHash(V1) = KeyHash(V2));
  V2.X := 2;
  WriteLn('other-typed Equal differs: ', KeyEquals(V1, V2));
  H1.A.Deg := 10;
  H2.A.Deg := 370;
  H1.N := 1;
  H2.N := 1;
  WriteLn('nested Equal: ', KeyEquals(H1, H2), ' ', KeyHash(H1) = KeyHash(H2));
  H2.N := 2;
  WriteLn('nested Equal, other field: ', KeyEquals(H1, H2));
  K := nil;
  WriteLn('class ref nil: ', KeyEquals<TFooClass>(nil, K), ' ', KeyEquals<TFooClass>(K, TFoo));
  Z := 0;
  WriteLn('nil hashes: ', KeyHash<TFoo>(nil) = KeyHash(Z), ' ', KeyHash<TProcP>(nil) = KeyHash(Z),
    ' ', KeyHash<TNotify>(nil) = KeyHash(Z), ' ', KeyHash<TAction>(nil) = KeyHash(Z), ' ',
    KeyHash<TFooClass>(nil) = KeyHash(Z));
  WriteLn('string inferred: ', KeyHash('abc') = KeyHash<string>('abc'));
end.
