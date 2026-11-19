program IntConstNegSizeOfCalcType;

// Unary minus on a constant gives a constant typed from its value, and SizeOf is an Integer
// constant typed from its value (semantics.md 3.2): both adapt to the other operand's type, so
// the arithmetic below stays in Integer / Cardinal and overflows under {$Q+}, as in Delphi XE4
// (M4-138). FPC computes these in Int64, so this is not a diff/ case

{$Q+}

uses System.SysUtils;

const
  NI64 = -Int64(1);
  SZ = SizeOf(Integer);

var
  V: Integer;
  C: Cardinal;
  U: UInt64;

begin
  V := MaxInt;
  try
    WriteLn(V - (-Int64(1)));
  except
    on E: EIntOverflow do
      WriteLn('overflow');
  end;
  try
    WriteLn(V - NI64);
  except
    on E: EIntOverflow do
      WriteLn('overflow');
  end;
  try
    WriteLn(V + SizeOf(Integer));
  except
    on E: EIntOverflow do
      WriteLn('overflow');
  end;
  C := High(Cardinal);
  try
    WriteLn(C + SizeOf(Integer));
  except
    on E: EIntOverflow do
      WriteLn('overflow');
  end;
  try
    WriteLn(C + SZ);
  except
    on E: EIntOverflow do
      WriteLn('overflow');
  end;
  // SizeOf adapts to UInt64 like a literal
  U := High(UInt64) - 4;
  WriteLn(U + SizeOf(Integer));
  // The negated value decides the type: Int64 when it does not fit in Integer
  WriteLn(-High(Cardinal));
  WriteLn(-Int64(5000000000));
  WriteLn(-Cardinal(1));
  // Delphi rejects negating a UInt64 constant; Puy accepts it when the result fits in Int64
  WriteLn(-UInt64(1));
end.
