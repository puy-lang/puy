program KeyEqualsTypes;

// KeyEquals / KeyHash on each kind of type (equality.md 2.1, 4): equal values are equal keys
// with the same hash, and different values are not equal. The hashes change between processes,
// so only their comparisons are printed. The type argument is written or inferred, and the names
// can be qualified with `System`.

type
  TColor = (Red, Green, Blue);
  TDigit = 0..9;
  TSmall = set of TColor;
  TBig = set of Char;
  TTriple = array[1..3] of Integer;
  TName = record
    First, Last: string;
  end;
  TFoo = class
  end;
  TFooClass = class of TFoo;

procedure Show(const What: string; Eq, Ne, SameHash: Boolean);
begin
  WriteLn(What, ': ', Eq, ' ', Ne, ' ', SameHash);
end;

var
  I1, I2: Integer;
  U1, U2: UInt64;
  C1, C2: TColor;
  D1, D2: TDigit;
  S1, S2: TSmall;
  B1, B2: TBig;
  Cu1, Cu2: Currency;
  Str1, Str2: string;
  U81, U82: UTF8String;
  T1, T2: TTriple;
  A1, A2: TArray<Integer>;
  N1, N2: TName;
  F1, F2: TFoo;
  K1, K2: TFooClass;
begin
  I1 := 7;
  I2 := 7;
  Show('Integer', KeyEquals(I1, I2), KeyEquals(I1, 8), KeyHash(I1) = KeyHash(I2));
  U1 := High(UInt64);
  U2 := U1;
  Show('UInt64', KeyEquals(U1, U2), KeyEquals<UInt64>(U1, 0), KeyHash(U1) = KeyHash(U2));
  Show('Boolean', KeyEquals(True, True), KeyEquals(True, False),
    KeyHash(True) = KeyHash(not False));
  Show('Char', KeyEquals('a', 'a'), KeyEquals('a', 'b'), KeyHash('a') = KeyHash(Chr(97)));
  C1 := TColor.Green;
  C2 := TColor.Green;
  Show('enum', KeyEquals(C1, C2), KeyEquals(C1, TColor.Blue), KeyHash(C1) = KeyHash(C2));
  D1 := 3;
  D2 := 3;
  Show('subrange', KeyEquals(D1, D2), KeyEquals<TDigit>(D1, 4), KeyHash(D1) = KeyHash(D2));
  S1 := [TColor.Red, TColor.Blue];
  S2 := [TColor.Blue, TColor.Red];
  Show('small set', KeyEquals(S1, S2), KeyEquals<TSmall>(S1, [TColor.Red]),
    KeyHash(S1) = KeyHash(S2));
  B1 := ['a', 'z'];
  B2 := ['z'];
  Include(B2, 'a');
  Show('big set', KeyEquals(B1, B2), KeyEquals<TBig>(B1, ['a']), KeyHash(B1) = KeyHash(B2));
  Cu1 := 1.5;
  Cu2 := 3 / 2;
  Show('Currency', KeyEquals(Cu1, Cu2), KeyEquals<Currency>(Cu1, 1.25),
    KeyHash(Cu1) = KeyHash(Cu2));
  Str1 := 'abc';
  Str2 := 'ab';
  Str2 := Str2 + 'c';
  Show('string', KeyEquals(Str1, Str2), KeyEquals(Str1, 'ABC'), KeyHash(Str1) = KeyHash(Str2));
  U81 := 'héllo';
  U82 := UTF8String('hé') + UTF8String('llo');
  Show('UTF8String', KeyEquals(U81, U82), KeyEquals<UTF8String>(U81, 'hello'),
    KeyHash(U81) = KeyHash(U82));
  T1[1] := 1;
  T1[2] := 2;
  T1[3] := 3;
  T2 := T1;
  Show('static array', KeyEquals(T1, T2), False, KeyHash(T1) = KeyHash(T2));
  T2[3] := 4;
  WriteLn('static array changed: ', KeyEquals(T1, T2));
  A1 := [1, 2, 3];
  A2 := Copy(A1);
  Show('dynamic array', KeyEquals(A1, A2), KeyEquals<TArray<Integer>>(A1, [3, 2, 1]),
    KeyHash(A1) = KeyHash(A2));
  A1 := nil;
  A2 := [];
  WriteLn('empty arrays: ', KeyEquals(A1, A2), ' ', KeyHash(A1) = KeyHash(A2));
  // Strings in a record are compared by content (equality.md 2.5)
  N1.First := 'Ada';
  N1.Last := 'Lovelace';
  N2 := N1;
  N2.First := 'A';
  N2.First := N2.First + 'da';
  Show('record', KeyEquals(N1, N2), False, KeyHash(N1) = KeyHash(N2));
  N2.Last := 'Byron';
  WriteLn('record changed: ', KeyEquals<TName>(N1, N2));
  // A class without Equals is compared by identity (equality.md 2.6)
  F1 := TFoo.Create;
  F2 := TFoo.Create;
  Show('class', KeyEquals(F1, F1), KeyEquals(F1, F2), KeyHash(F1) = KeyHash(F1));
  WriteLn('class nil: ', KeyEquals<TFoo>(nil, nil), ' ', KeyEquals<TFoo>(F1, nil));
  K1 := TFoo;
  K2 := TFoo;
  Show('class reference', KeyEquals(K1, K2), KeyEquals<TClass>(TFoo, TObject),
    KeyHash(K1) = KeyHash(K2));
  // The names qualified with `System`
  WriteLn('System: ', System.KeyEquals<Integer>(I1, 7), ' ', System.KeyEquals(I1, I2), ' ',
    System.KeyHash(I1) = System.KeyHash<Integer>(I2));
  F1.Free;
  F2.Free;
end.
