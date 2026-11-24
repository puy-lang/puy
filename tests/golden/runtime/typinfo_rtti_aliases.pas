program TypInfoRttiAliases;

// The RTTI exports share the same type identity, even when listed before their defining unit.
uses System.Rtti, System.TypInfo;

var
  Context: TRttiContext;
  Visibility: TMemberVisibility;
  Flags: TParamFlags;
begin
  Visibility := System.Rtti.TMemberVisibility.mvPublished;
  Flags := [System.Rtti.TParamFlag.pfConst];
  WriteLn(Visibility = System.TypInfo.TMemberVisibility.mvPublished, ' ',
    System.TypInfo.TParamFlag.pfConst in Flags);
  WriteLn(Ord(High(TMemberVisibility)), ' ', Ord(Low(TMethodKind)));
  WriteLn(TypeInfo(TMemberVisibility) = TypeInfo(System.TypInfo.TMemberVisibility), ' ',
    TypeInfo(TMethodKind) = TypeInfo(System.Rtti.TMethodKind), ' ',
    TypeInfo(TParamFlag) = TypeInfo(System.Rtti.TParamFlag), ' ',
    TypeInfo(TParamFlags) = TypeInfo(System.Rtti.TParamFlags), ' ',
    TypeInfo(TOrdType) = TypeInfo(System.Rtti.TOrdType), ' ',
    TypeInfo(TFloatType) = TypeInfo(System.Rtti.TFloatType));
  Context := TRttiContext.Create;
  WriteLn(Context.GetType(TypeInfo(System.Rtti.TMemberVisibility)).QualifiedName);
  WriteLn(Context.GetType(TypeInfo(System.Rtti.TParamFlags)).QualifiedName);
end.
