program KeyEqualsFloatsStrings;

// The key rules that differ from `=` (equality.md 2.2, 2.3): 0.0 and -0.0 are equal keys with the
// same hash, NaN equals NaN whatever its bits, while `=` stays IEEE 754; a string is hashed by
// its content whether it is held one byte or two bytes per code unit.

var
  Z, NZ, NaN1, NaN2, Inf: Double;
  SZ, SNZ, SNaN: Single;
  S1, S2: string;
begin
  Z := 0.0;
  NZ := -Z;
  WriteLn('0.0 = -0.0: ', Z = NZ, ' ', KeyEquals(Z, NZ), ' ', KeyHash(Z) = KeyHash(NZ));
  NaN1 := Z / Z;
  NaN2 := -NaN1;
  WriteLn('NaN = NaN: ', NaN1 = NaN1, ' ', KeyEquals(NaN1, NaN2), ' ',
    KeyHash(NaN1) = KeyHash(NaN2));
  WriteLn('NaN and 0.0: ', KeyEquals(NaN1, Z));
  Inf := 1 / Z;
  WriteLn('INF and -INF: ', KeyEquals(Inf, -Inf), ' ', KeyEquals(Inf, 1 / Z));
  SZ := 0.0;
  SNZ := -SZ;
  SNaN := SZ / SZ;
  WriteLn('Single: ', KeyEquals(SZ, SNZ), ' ', KeyHash(SZ) = KeyHash(SNZ), ' ',
    KeyEquals(SNaN, SNaN), ' ', KeyEquals<Single>(SZ, 1.5));
  WriteLn('literals: ', KeyEquals(1.5, 1.5), ' ', KeyEquals(0.0, -0.0));
  // `S1` is held one byte per code unit; `S2` keeps two bytes per code unit after its non-ASCII
  // character is replaced in place
  S1 := 'abc';
  S2 := 'あbc';
  S2[1] := 'a';
  WriteLn('representations: ', S1 = S2, ' ', KeyEquals(S1, S2), ' ', KeyHash(S1) = KeyHash(S2));
  WriteLn('empty: ', KeyHash('') = KeyHash(Copy(S1, 1, 0)));
  WriteLn('case: ', KeyEquals(S1, 'ABC'));
end.
