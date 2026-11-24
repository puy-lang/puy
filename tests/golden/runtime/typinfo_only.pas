program TypInfoOnly;

// The enumeration and set types are available without importing System.Rtti.
uses System.TypInfo;

var
  Flags: TParamFlags;
  Info: System.PTypeInfo;
begin
  Info := TypeInfo(Integer);
  WriteLn(Info = TypeInfo(Integer));
  Flags := [TParamFlag.pfConst, TParamFlag.pfOut];
  WriteLn(Ord(TMemberVisibility.mvPublished), ' ', Ord(TMethodKind.mkProcedure), ' ',
    Ord(TOrdType.otULong), ' ', Ord(TFloatType.ftCurr));
  WriteLn(TParamFlag.pfConst in Flags, ' ', TParamFlag.pfOut in Flags, ' ',
    TParamFlag.pfVar in Flags);
end.
