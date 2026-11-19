program TypeBuiltinNestedType;

// The built-ins that take a type (`Default`, `SizeOf`, `Low`, `High`, `KeyHash<T>`) accept a
// nested type written with its outer type (`TC.TR`), with an instantiation (`TGen<Integer>.TI`)
// and with an alias of an instantiation (`TIntGen.TI`) (semantics.md 5, equality.md 4,
// generics.md 2.5). Each gives the same result as through an alias of the nested type.

type
  TC = class
  public
    type
      TR = record
        X: Integer;
        S: string;
      end;
      TE = (cA, cB, cC, cD);
      TS = 2..7;
    const
      K: array[0..2] of Integer = (1, 2, 3);
  end;

  TP = record
  public
    type
      TE = (pA, pB, pC);
  end;

  TGen<T> = class
  public
    type
      TI = record
        V: T;
        N: Integer;
      end;
      TE = (gA, gB);
  end;

  TIntGen = TGen<Integer>;

  TAR = TC.TR;
  TAE = TC.TE;
  TAS = TC.TS;
  TAPE = TP.TE;
  TAI = TGen<Integer>.TI;
  TAGE = TGen<Integer>.TE;

  TBounds = record
    Items: array[3..9] of Integer;
  end;

var
  R: TC.TR;
  I: TGen<Integer>.TI;
  J: TIntGen.TI;
  B: TBounds;
begin
  R := Default(TC.TR);
  WriteLn(R.X, '[', R.S, '] ', Default(TAR).X, '[', Default(TAR).S, ']');
  WriteLn(SizeOf(TC.TE), ' ', SizeOf(TAE), ' ', SizeOf(TC.TS), ' ', SizeOf(TAS));
  WriteLn(Ord(Low(TC.TE)), ' ', Ord(High(TC.TE)), ' ', Ord(Low(TAE)), ' ', Ord(High(TAE)));
  WriteLn(Low(TC.TS), ' ', High(TC.TS), ' ', Low(TAS), ' ', High(TAS));
  WriteLn(Ord(High(TP.TE)), ' ', Ord(High(TAPE)), ' ', High(TP.TE) = TAPE.pC);
  I := Default(TGen<Integer>.TI);
  WriteLn(I.V, ' ', I.N, ' ', Default(TAI).V);
  J := Default(TIntGen.TI);
  WriteLn(J.V, ' ', J.N);
  WriteLn(Ord(High(TGen<Integer>.TE)), ' ', Ord(High(TAGE)), ' ', Ord(High(TIntGen.TE)));
  I.V := 42;
  I.N := 7;
  WriteLn(KeyHash<TGen<Integer>.TI>(I) = KeyHash<TAI>(I));
  WriteLn(KeyHash<TIntGen.TI>(I) = KeyHash<TAI>(I));
  // A field of a value, and a constant of a type, are still values
  WriteLn(Low(B.Items), ' ', High(B.Items));
  WriteLn(Low(TC.K), ' ', High(TC.K));
end.
