program TypInfoUnavailable;

// Unavailable TypInfo names suggest RTTI alternatives, whether used as calls or type names.
uses System.TypInfo;

var
  Prop: System.TypInfo.PPropInfo;
  Data: PTypeData;
  Kind: System.TypInfo.TTypeKind;
  Info: System.TypInfo.PTypeInfo;
begin
  WriteLn(System.TypInfo.getenumname(TypeInfo(Integer), 0));
  WriteLn(GetEnumValue(TypeInfo(Integer), 'abc'));
  WriteLn(GetTypeData(TypeInfo(Integer)));
  WriteLn(GetPropInfo(TypeInfo(Integer), 'Text'));
  WriteLn(GetPropValue(nil, 'Text'));
  System.TypInfo.SetPropValue(nil, 'Text', 0);
  WriteLn(Ord(System.TypInfo.tkClass));
end.
