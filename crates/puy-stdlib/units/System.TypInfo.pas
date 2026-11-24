unit System.TypInfo;

interface

type
  // Keep the ordinals from docs/spec/rtti.md 5.1, including values Puy does not produce.
  TMemberVisibility = (mvPrivate, mvProtected, mvPublic, mvPublished);
  TMethodKind = (mkProcedure, mkFunction, mkConstructor, mkDestructor, mkClassProcedure,
    mkClassFunction, mkClassConstructor, mkClassDestructor, mkOperatorOverload, mkSafeProcedure,
    mkSafeFunction);
  TParamFlag = (pfVar, pfConst, pfArray, pfAddress, pfReference, pfOut, pfResult);
  TParamFlags = set of TParamFlag;
  TOrdType = (otSByte, otUByte, otSWord, otUWord, otSLong, otULong);
  TFloatType = (ftSingle, ftDouble, ftExtended, ftComp, ftCurr);

implementation

end.
