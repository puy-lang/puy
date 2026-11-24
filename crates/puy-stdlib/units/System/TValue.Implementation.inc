// ===== TValue (docs/spec/rtti.md chapter 3) =====

// The modes of ValueRead (docs/spec/bytecode.md 2.24, I123).
const
  ValueReadInteger = 0;
  ValueReadInt64 = 1;
  ValueReadOrdinal = 2;
  ValueReadBoolean = 3;
  ValueReadEmpty = 6;

class function TValue.GetEmpty: TValue;
begin
  Result := Default(TValue);
end;

// Empty, or made from nil of a class, class reference, interface or PTypeInfo (3.2).
function TValue.GetIsEmpty: Boolean;
begin
  Result := ValueRead(FType, FBits, FRef, ValueReadEmpty) <> 0;
end;

function TValue.GetKind: TTypeKind;
begin
  Result := TTypeKind(TypeKindOf(FType));
end;

function TValue.GetTypeInfo: PTypeInfo;
begin
  Result := TypeInfoOfNumber(FType);
end;

class function TValue.From<T>(const AValue: T): TValue;
begin
  Result := ValueWrap(AValue, System.TypeInfo(T));
end;

function TValue.AsType<T>: T;
begin
  ValueIs(T, Self, True);
  Result := ValueCast(T, Self);
end;

function TValue.TryAsType<T>(out AResult: T): Boolean;
begin
  Result := ValueIs(T, Self, False);
  if Result then
    AResult := ValueCast(T, Self)
  else
    AResult := Default(T);
end;

function TValue.IsType<T>: Boolean;
begin
  Result := ValueIs(T, Self, False);
end;

function TValue.IsObject: Boolean;
begin
  Result := GetKind = TTypeKind.tkClass;
end;

function TValue.IsClass: Boolean;
begin
  Result := GetKind = TTypeKind.tkClassRef;
end;

function TValue.IsOrdinal: Boolean;
begin
  Result := GetKind in [TTypeKind.tkInteger, TTypeKind.tkInt64, TTypeKind.tkWChar,
    TTypeKind.tkEnumeration];
end;

function TValue.IsArray: Boolean;
begin
  Result := GetKind in [TTypeKind.tkArray, TTypeKind.tkDynArray];
end;

function TValue.AsInteger: Integer;
begin
  Result := Integer(ValueRead(FType, FBits, FRef, ValueReadInteger));
end;

function TValue.AsInt64: Int64;
begin
  Result := ValueRead(FType, FBits, FRef, ValueReadInt64);
end;

function TValue.AsOrdinal: Int64;
begin
  Result := ValueRead(FType, FBits, FRef, ValueReadOrdinal);
end;

function TValue.AsBoolean: Boolean;
begin
  Result := ValueRead(FType, FBits, FRef, ValueReadBoolean) <> 0;
end;

function TValue.AsExtended: Double;
begin
  Result := ValueReadDouble(FType, FBits, FRef);
end;

function TValue.AsCurrency: Currency;
begin
  Result := ValueReadCurrency(FType, FBits, FRef);
end;

function TValue.AsString: string;
begin
  Result := ValueReadString(FType, FBits, FRef);
end;

function TValue.AsObject: TObject;
begin
  Result := AsType<TObject>;
end;

function TValue.AsClass: TClass;
begin
  Result := AsType<TClass>;
end;

function TValue.AsInterface: IInterface;
begin
  Result := AsType<IInterface>;
end;

function TValue.GetArrayLength: Integer;
begin
  Result := ValueArrayLength(FType, FRef);
end;

// An element that is itself a TValue is read and written as it is (not nested; 3.3).
function TValue.GetArrayElement(Index: Integer): TValue;
begin
  if ValueArrayElemType(FType, FRef, Index) = TypeInfoNumber(System.TypeInfo(TValue)) then
    Exit(ValueArrayValue(FType, FRef, Index));
  Result := Default(TValue);
  Result.FType := ValueArrayType(FType, FRef, Index);
  Result.FBits := ValueArrayBits(FType, FRef, Index);
  Result.FRef := ValueArrayRef(FType, FRef, Index);
end;

procedure TValue.SetArrayElement(Index: Integer; const AValue: TValue);
begin
  var Elem := ValueArrayElemType(FType, FRef, Index);
  if Elem = TypeInfoNumber(System.TypeInfo(TValue)) then
    FRef := ValueArraySetValue(FType, FRef, Index, AValue)
  else
  begin
    ValueMatch(AValue.FType, AValue.FBits, AValue.FRef, Elem, True);
    FRef := ValueArraySet(FType, FRef, Index, AValue.FBits, AValue.FRef);
  end;
end;

function TValue.ToString: string;
begin
  Result := ValueToString(FType, FBits, FRef);
end;

class operator TValue.Implicit(const Value: string): TValue;
begin
  Result := ValueWrap(Value, System.TypeInfo(string));
end;

class operator TValue.Implicit(Value: Integer): TValue;
begin
  Result := ValueWrap(Value, System.TypeInfo(Integer));
end;

class operator TValue.Implicit(Value: Cardinal): TValue;
begin
  Result := ValueWrap(Value, System.TypeInfo(Cardinal));
end;

class operator TValue.Implicit(Value: Int64): TValue;
begin
  Result := ValueWrap(Value, System.TypeInfo(Int64));
end;

class operator TValue.Implicit(Value: UInt64): TValue;
begin
  Result := ValueWrap(Value, System.TypeInfo(UInt64));
end;

class operator TValue.Implicit(Value: Boolean): TValue;
begin
  Result := ValueWrap(Value, System.TypeInfo(Boolean));
end;

class operator TValue.Implicit(Value: Double): TValue;
begin
  Result := ValueWrap(Value, System.TypeInfo(Double));
end;

class operator TValue.Implicit(Value: Currency): TValue;
begin
  Result := ValueWrap(Value, System.TypeInfo(Currency));
end;

class operator TValue.Implicit(Value: TObject): TValue;
begin
  Result := ValueWrap(Value, System.TypeInfo(TObject));
end;

class operator TValue.Implicit(Value: TClass): TValue;
begin
  Result := ValueWrap(Value, System.TypeInfo(TClass));
end;

// The ordinal AValue of the ordinal type ATypeInfo (docs/spec/rtti.md 3.3): EInvalidCast for
// another type, ERangeError for a value out of the type's range.
class function TValue.FromOrdinal(ATypeInfo: PTypeInfo; AValue: Int64): TValue;
const
  FieldName = 2;
  FieldShape = 5;
  FieldMin = 6;
  FieldMax = 7;
  ShapeUInt64 = 7;
begin
  var N := TypeInfoNumber(ATypeInfo);
  var TypeName := '';
  if N <> 0 then
    TypeName := RttiName(Integer(RttiTypeField(ATypeInfo, FieldName, 0)));
  if not (TTypeKind(TypeKindOf(N)) in [TTypeKind.tkInteger, TTypeKind.tkInt64,
    TTypeKind.tkWChar, TTypeKind.tkEnumeration]) then
    raise EInvalidCast.Create('invalid type cast: ordinal to `' + TypeName + '`');
  var Lo := RttiTypeField(ATypeInfo, FieldMin, 0);
  var Hi := RttiTypeField(ATypeInfo, FieldMax, 0);
  if (RttiTypeField(ATypeInfo, FieldShape, 0) <> ShapeUInt64) and ((AValue < Lo) or (AValue > Hi)) then
    raise ERangeError.Create('value ' + IntToStr(AValue) + ' is out of range for ' + TypeName +
      ' (' + IntToStr(Lo) + '..' + IntToStr(Hi) + ')');
  Result := Default(TValue);
  Result.FType := N;
  Result.FBits := AValue;
end;
