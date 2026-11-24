program RttiTypeKinds;

// The values of TTypeKind have Delphi's ordinals, the ones Puy's types never take included
// (rtti.md 2.3), and TypeInfo of a type is the same value wherever it is written (2.2)

uses System.TypInfo, System.Rtti;

type
  TColor = (Red, Green, Blue);

begin
  WriteLn(Ord(TTypeKind.tkUnknown), ' ', Ord(TTypeKind.tkInteger), ' ', Ord(TTypeKind.tkChar), ' ',
    Ord(TTypeKind.tkEnumeration), ' ', Ord(TTypeKind.tkFloat), ' ', Ord(TTypeKind.tkString));
  WriteLn(Ord(TTypeKind.tkSet), ' ', Ord(TTypeKind.tkClass), ' ', Ord(TTypeKind.tkMethod), ' ',
    Ord(TTypeKind.tkWChar), ' ', Ord(TTypeKind.tkLString), ' ', Ord(TTypeKind.tkWString));
  WriteLn(Ord(TTypeKind.tkVariant), ' ', Ord(TTypeKind.tkArray), ' ', Ord(TTypeKind.tkRecord), ' ',
    Ord(TTypeKind.tkInterface), ' ', Ord(TTypeKind.tkInt64), ' ', Ord(TTypeKind.tkDynArray));
  WriteLn(Ord(TTypeKind.tkUString), ' ', Ord(TTypeKind.tkClassRef), ' ',
    Ord(TTypeKind.tkPointer), ' ', Ord(TTypeKind.tkProcedure));
  WriteLn(Ord(Low(TTypeKind)), ' ', Ord(High(TTypeKind)));
  WriteLn(TypeInfo(Integer) = TypeInfo(Integer));
  WriteLn(TypeInfo(Integer) = TypeInfo(Byte));
  WriteLn(TypeInfo(TColor) <> nil);
  WriteLn(Assigned(TypeInfo(string)));
end.
