program IntCastConstCalcType;

// A constant given its type by a conversion (`Int64(1)`) keeps that type in the computation-type
// table (semantics.md 3.2); only a constant typed by its value adapts to the other operand's type,
// including a constant folded from such constants (M4-136, checked against Delphi XE4).
// Before the fix `V + Int64(1)` was computed in Integer and raised EIntOverflow under {$Q+}

{$Q+}

uses System.SysUtils;

const
  N = Int64(1);
  M = Int64(1) + 1;
  K = 4000000000 - 3999999999;
  CC = Cardinal(5) + 1;
  T: Integer = 1;

type
  TFoo = class
  const
    K = Integer(1);
    L: Integer = 1;
  end;

var
  V: Integer;
  C: Cardinal;
  U: UInt64;

begin
  V := MaxInt;
  WriteLn(V + Int64(1));
  WriteLn(Int64(V) + 1);
  WriteLn(Int64(1) + V);
  WriteLn(V - Int64(-1));
  WriteLn(V * Int64(2));
  WriteLn(V + N);
  WriteLn(V + M);
  // Cardinal with a signed type is computed in Int64
  WriteLn(V + Cardinal(1));
  V := -MaxInt - 1;
  WriteLn(V - Cardinal(1));
  // Cardinal with Int64 is computed in Int64
  C := High(Cardinal);
  WriteLn(C + Int64(1));
  // Cardinal with Cardinal stays Cardinal, as with the literal
  try
    WriteLn(C + Cardinal(1));
  except
    on E: EIntOverflow do
      WriteLn('overflow');
  end;
  try
    WriteLn(C + 1);
  except
    on E: EIntOverflow do
      WriteLn('overflow');
  end;
  // UInt64 with an unsigned type stays UInt64
  U := High(UInt64) - 1;
  WriteLn(U + UInt64(1));
  WriteLn(U + Cardinal(1));
  // A conversion keeps its type even when it is the literal's own type: C + Integer(1) and
  // C + Int64(3000000000) are computed in Int64, as in Delphi (in Cardinal they would overflow)
  C := High(Cardinal);
  WriteLn(C + Int64(3000000000));
  WriteLn(C + Integer(1));
  // So does a class constant whose type was written
  WriteLn(C + TFoo.K);
  WriteLn(C + TFoo.L);
  WriteLn(C + High(Integer));
  WriteLn(C + T);
  // A constant folded from constants typed by their values takes its type from its value, so it
  // adapts like a literal: K and CC are the Integer constants 1 and 6
  try
    WriteLn(C + K);
  except
    on E: EIntOverflow do
      WriteLn('overflow');
  end;
  try
    WriteLn(C + CC);
  except
    on E: EIntOverflow do
      WriteLn('overflow');
  end;
  V := MaxInt;
  try
    WriteLn(V + K);
  except
    on E: EIntOverflow do
      WriteLn('overflow');
  end;
  U := High(UInt64) - 1;
  WriteLn(U + K);
  // A literal still adapts: Integer + 1 is computed in Integer
  V := MaxInt;
  try
    WriteLn(V + 1);
  except
    on E: EIntOverflow do
      WriteLn('overflow');
  end;
end.
