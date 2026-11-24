program TValueBasic;

// TValue made from values of several types: what it holds, its kind, ToString, the reads by
// name, and IsType / AsType of the same type (rtti.md chapter 3); `array of const` passed on to
// Format (4.2)

uses System.SysUtils, System.TypInfo, System.Rtti;

type
  TColor = (Red, Green, Blue);
  TBase = class
  end;
  TDerived = class(TBase)
  end;

function Fmt(const S: string; const Args: array of const): string;
begin
  Result := Format(S, Args);
end;

var
  V: TValue;
  D: TDerived;
begin
  V := TValue.From<Integer>(42);
  WriteLn(V.ToString, ' ', V.AsInteger, ' ', V.AsInt64, ' ', Ord(V.Kind), ' ', V.IsOrdinal);
  WriteLn(V.IsType<Integer>, ' ', V.IsType<string>, ' ', V.AsType<Integer>);
  V := TValue.From<string>('hello');
  WriteLn(V.ToString, ' ', V.AsString, ' ', Ord(V.Kind), ' ', V.IsOrdinal);
  V := TValue.From<TColor>(TColor.Green);
  WriteLn(V.ToString, ' ', Ord(V.Kind), ' ', V.AsOrdinal, ' ', V.AsType<TColor> = TColor.Green);
  V := TValue.From<Boolean>(True);
  WriteLn(V.ToString, ' ', V.AsBoolean, ' ', Ord(V.Kind));
  V := TValue.From<Double>(2.5);
  WriteLn(V.ToString, ' ', V.AsExtended:0:2, ' ', Ord(V.Kind));
  V := TValue.From<Char>('q');
  WriteLn(V.ToString, ' ', Ord(V.Kind));
  V := TValue.From<Int64>(5000000000);
  WriteLn(V.ToString, ' ', V.AsInt64, ' ', Ord(V.Kind));
  V := TValue.Empty;
  WriteLn(V.ToString, ' ', V.IsEmpty, ' ', Ord(V.Kind), ' ', V.TypeInfo = nil);
  D := TDerived.Create;
  V := TValue.From<TBase>(D);
  WriteLn(V.IsObject, ' ', V.IsEmpty, ' ', V.AsObject = D, ' ', V.TypeInfo = TypeInfo(TDerived));
  WriteLn(V.IsType<TBase>, ' ', V.IsType<TDerived>, ' ', V.AsType<TDerived> = D);
  D.Free;
  V := TValue.From<TBase>(nil);
  WriteLn(V.IsObject, ' ', V.IsEmpty, ' ', V.AsObject = nil);
  V := TValue.From<TClass>(TDerived);
  WriteLn(V.IsClass, ' ', V.AsClass.ClassName);
  V := 7;
  WriteLn(V.ToString, ' ', V.IsType<Integer>);
  V := 'text';
  WriteLn(V.ToString, ' ', V.IsType<string>);
  WriteLn(Fmt('%d-%s-%.1f', [3, 'x', 1.5]));
  WriteLn(Fmt('none', []));
end.
