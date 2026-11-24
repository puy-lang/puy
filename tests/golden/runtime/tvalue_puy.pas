program TValuePuy;

// TValue behaviours of Puy's own (rtti.md chapter 3): no widening in AsType, the class of an
// instance recorded, ToString without addresses, records, sets and arrays held, the
// EInvalidCast messages, a descendant class through Implicit(TObject) (objects.md 11.4), and
// arrays of TValue (their elements not nested, 3.3)

uses System.SysUtils;

type
  TPoint = record
    X, Y: Integer;
  end;
  TFlag = (fA, fB, fC);
  TFlags = set of TFlag;
  TSmall = 1..10;
  TCount = type Integer;
  TNames = array[0..2] of string;
  TFoo = class
  end;
  TValues = array[0..1] of TValue;

var
  V, W: TValue;
  P: TPoint;
  A: TArray<Integer>;
  N: TNames;
  F: TFoo;
  S: TSmall;
  B: Byte;
  VA: TArray<TValue>;
  VS: TValues;
  PA: TArray<TPoint>;
begin
  B := 5;
  V := TValue.From<Byte>(B);
  WriteLn(V.IsType<Byte>, ' ', V.IsType<Integer>, ' ', V.AsInteger);
  S := 3;
  V := TValue.From<TSmall>(S);
  WriteLn(V.IsType<TSmall>, ' ', V.IsType<Integer>);
  V := TValue.From<TCount>(4);
  WriteLn(V.IsType<TCount>, ' ', V.IsType<Integer>, ' ', V.AsInteger);
  P.X := 1;
  P.Y := 2;
  V := TValue.From<TPoint>(P);
  P.X := 99;
  WriteLn(V.ToString, ' ', V.AsType<TPoint>.X, ' ', Ord(V.Kind));
  V := TValue.From<TFlags>([TFlag.fA, TFlag.fC]);
  WriteLn(V.ToString, ' ', TFlag.fC in V.AsType<TFlags>);
  A := [10, 20, 30];
  V := TValue.From<TArray<Integer>>(A);
  WriteLn(V.IsArray, ' ', V.GetArrayLength, ' ', V.GetArrayElement(1).AsInteger);
  V.SetArrayElement(1, 25);
  WriteLn(A[1]);
  N[0] := 'a';
  N[1] := 'b';
  N[2] := 'c';
  V := TValue.From<TNames>(N);
  W := V;
  V.SetArrayElement(0, 'z');
  WriteLn(V.GetArrayElement(0).AsString, ' ', W.GetArrayElement(0).AsString, ' ', N[0]);
  try
    V.GetArrayElement(3);
  except
    on E: ERangeError do WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    V.SetArrayElement(0, 1);
  except
    on E: EInvalidCast do WriteLn(E.ClassName, ': ', E.Message);
  end;
  F := TFoo.Create;
  V := F;
  WriteLn(V.ToString, ' ', V.AsObject = F, ' ', TValue.From<TClass>(TFoo).ToString);
  try
    V.AsType<string>;
  except
    on E: EInvalidCast do WriteLn(E.ClassName, ': ', E.Message);
  end;
  F.Free;
  try
    WriteLn(TValue.From<Int64>(5000000000).AsInteger);
  except
    on E: EInvalidCast do WriteLn(E.ClassName, ': ', E.Message);
  end;
  try
    WriteLn(TValue.Empty.AsInteger);
  except
    on E: EInvalidCast do WriteLn(E.ClassName, ': ', E.Message);
  end;
  WriteLn(TValue.Empty.AsObject = nil, ' ', TValue.Empty.IsType<TFoo>, ' ', TValue.Empty.IsType<Integer>);
  V := TValue.From<Currency>(12.5);
  WriteLn(V.ToString, ' ', V.AsExtended:0:2, ' ', V.AsCurrency:0:2);
  V := TValue.From<UTF8String>('utf');
  WriteLn(V.ToString, ' ', V.AsString);
  V := TValue.From<TValue>(TValue.From<Integer>(1));
  WriteLn(V.IsType<Integer>, ' ', V.AsType<TValue>.AsInteger);
  // The elements of an array of TValue are read and written as they are, not nested
  VA := [42, 'x'];
  V := TValue.From<TArray<TValue>>(VA);
  WriteLn(V.GetArrayElement(0).ToString, ' ', V.GetArrayElement(0).IsType<Integer>);
  V.SetArrayElement(1, 7);
  WriteLn(VA[1].ToString);
  VS[0] := 'a';
  VS[1] := 1.5;
  V := TValue.From<TValues>(VS);
  V.SetArrayElement(0, True);
  WriteLn(V.GetArrayElement(0).ToString, ' ', V.GetArrayElement(1).ToString, ' ', VS[0].ToString);
  // Records laid out flat in a dynamic array: read as a box, written back into the array
  SetLength(PA, 2);
  PA[1].X := 2;
  V := TValue.From<TArray<TPoint>>(PA);
  WriteLn(V.GetArrayElement(1).AsType<TPoint>.X, ' ', V.GetArrayElement(0).ToString);
  P.X := 9;
  V.SetArrayElement(0, TValue.From<TPoint>(P));
  WriteLn(PA[0].X);
end.
