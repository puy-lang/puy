unit System.Rtti;

// The run-time type information API (docs/spec/rtti.md chapter 5, docs/spec/stdlib/System.Rtti/
// README.md), written over the type information section the compiler puts in the module
// (docs/spec/bytecode.md 3.2) and the built-ins that read it (2.24). Written from the
// specification only, not from any other implementation. It does not use System.SysUtils:
// reading a unit costs the program that uses this one.
//
// A TRttiType and a member are objects of reference counted classes, never freed by hand. Each
// type and each member has one object per isolate: the global arrays below keep them, by type
// information number and by the index of the member's entry, until the isolate ends. The objects
// hold numbers, not one another, so no cycle keeps them alive. Only the routines a program
// reaches are kept, and which ones decide what the section holds (docs/spec/rtti.md 1.5.1): the
// members are read only by the routines that list them (RttiMemberField), and the types are
// counted only by GetTypes and FindType (RttiTypeCount, RttiFindType).

interface

uses System.TypInfo;

type
  // The same type as System.TValue (docs/spec/rtti.md chapter 3).
  TValue = System.TValue;

  // Re-export the same types for code using this unit alone (docs/spec/rtti.md 5.1).
  TMemberVisibility = System.TypInfo.TMemberVisibility;
  TMethodKind = System.TypInfo.TMethodKind;
  TParamFlag = System.TypInfo.TParamFlag;
  TParamFlags = System.TypInfo.TParamFlags;
  TOrdType = System.TypInfo.TOrdType;
  TFloatType = System.TypInfo.TFloatType;

  TRttiType = class;
  TRttiField = class;
  TRttiProperty = class;
  TRttiMethod = class;
  TRttiParameter = class;
  TRttiInstanceType = class;
  TRttiRecordType = class;
  TRttiOrdinalType = class;
  TRttiSetType = class;
  TRttiInterfaceType = class;

  TRttiObject = class(TRefObject)
  private
    // The range of the attributes table of the object's entry (docs/spec/rtti.md 5.8). Not
    // virtual: a table entry would keep it in every program making these objects.
    procedure AttrRange(out First, Count: Integer);
    // The bodies using the built-ins of the bridges and the attributes' functions are methods:
    // the analysis of a method of the standard library waits until a body uses it, and only
    // then makes those functions (docs/spec/rtti.md 1.5).
    class function AttrAt(I: Integer): TCustomAttribute; static;
  public
    // The instances of the attributes written on the declaration, in the order written; each
    // made once per isolate, when first asked for (5.8).
    function GetAttributes: TArray<TCustomAttribute>;
    // The first attribute that is a T, nil if none.
    function GetAttribute<T: TCustomAttribute>: T;
    function HasAttribute<T: TCustomAttribute>: Boolean;
  end;

  TRttiNamedObject = class(TRttiObject)
  private
    FName: string;
  public
    property Name: string read FName;
  end;

  TRttiType = class(TRttiNamedObject)
  private
    FHandle: PTypeInfo;
    function GetTypeKind: TTypeKind;
    function GetQualifiedName: string;
    function GetBaseType: TRttiType;
    function GetIsInstance: Boolean;
    function GetAsInstance: TRttiInstanceType;
    function GetIsRecord: Boolean;
    function GetAsRecord: TRttiRecordType;
    function GetIsOrdinal: Boolean;
    function GetAsOrdinal: TRttiOrdinalType;
    function GetIsSet: Boolean;
    function GetAsSet: TRttiSetType;
    function GetGenericDefinitionName: string;
    function NotA(const What: string): EInvalidCast;
  public
    function GetFields: TArray<TRttiField>;
    function GetField(const AName: string): TRttiField;
    function GetDeclaredFields: TArray<TRttiField>;
    function GetProperties: TArray<TRttiProperty>;
    function GetProperty(const AName: string): TRttiProperty;
    function GetDeclaredProperties: TArray<TRttiProperty>;
    function GetMethods: TArray<TRttiMethod>; overload;
    function GetMethods(const AName: string): TArray<TRttiMethod>; overload;
    function GetMethod(const AName: string): TRttiMethod;
    function GetDeclaredMethods: TArray<TRttiMethod>;
    // Puy's own (5.3): the type arguments of an instantiation, in the order written.
    function GetGenericArguments: TArray<TRttiType>;
    function ToString: string; override;
    property QualifiedName: string read GetQualifiedName;
    property TypeKind: TTypeKind read GetTypeKind;
    property Handle: PTypeInfo read FHandle;
    property BaseType: TRttiType read GetBaseType;
    property IsInstance: Boolean read GetIsInstance;
    property AsInstance: TRttiInstanceType read GetAsInstance;
    property IsRecord: Boolean read GetIsRecord;
    property AsRecord: TRttiRecordType read GetAsRecord;
    property IsOrdinal: Boolean read GetIsOrdinal;
    property AsOrdinal: TRttiOrdinalType read GetAsOrdinal;
    property IsSet: Boolean read GetIsSet;
    property AsSet: TRttiSetType read GetAsSet;
    // Puy's own (5.3): the fully qualified name of an instantiation's definition, '' otherwise.
    property GenericDefinitionName: string read GetGenericDefinitionName;
  end;

  TRttiInstanceType = class(TRttiType)
  private
    function GetMetaclassType: TClass;
    function GetInstanceBaseType: TRttiInstanceType;
  public
    property MetaclassType: TClass read GetMetaclassType;
    property BaseType: TRttiInstanceType read GetInstanceBaseType;
  end;

  TRttiRecordType = class(TRttiType)
  end;

  TRttiOrdinalType = class(TRttiType)
  private
    function GetOrdType: TOrdType;
    function GetMinValue: Int64;
    function GetMaxValue: Int64;
  public
    property OrdType: TOrdType read GetOrdType;
    property MinValue: Int64 read GetMinValue;
    property MaxValue: Int64 read GetMaxValue;
  end;

  TRttiEnumerationType = class(TRttiOrdinalType)
  private
    function GetUnderlyingType: TRttiType;
  public
    function GetNames: TArray<string>;
    // Puy's own (5.4): the value of the enumeration T named AName (ignoring case);
    // EArgumentException if none.
    class function GetValue<T>(const AName: string): T;
    // Puy's own (5.4): the name of the value AValue of the enumeration T.
    class function GetName<T>(AValue: T): string;
    property UnderlyingType: TRttiType read GetUnderlyingType;
  end;

  TRttiFloatType = class(TRttiType)
  private
    function GetFloatType: TFloatType;
  public
    property FloatType: TFloatType read GetFloatType;
  end;

  TRttiStringType = class(TRttiType)
  end;

  TRttiSetType = class(TRttiType)
  private
    function GetElementType: TRttiType;
  public
    property ElementType: TRttiType read GetElementType;
  end;

  TRttiArrayType = class(TRttiType)
  private
    function GetElementType: TRttiType;
    function GetTotalElementCount: Integer;
    function GetDimensionCount: Integer;
  public
    property ElementType: TRttiType read GetElementType;
    property TotalElementCount: Integer read GetTotalElementCount;
    // Always 1: an array of more dimensions is an array of arrays.
    property DimensionCount: Integer read GetDimensionCount;
  end;

  TRttiDynamicArrayType = class(TRttiType)
  private
    function GetElementType: TRttiType;
  public
    property ElementType: TRttiType read GetElementType;
  end;

  TRttiInterfaceType = class(TRttiType)
  private
    function GetInterfaceBaseType: TRttiInterfaceType;
  public
    property BaseType: TRttiInterfaceType read GetInterfaceBaseType;
  end;

  TRttiClassRefType = class(TRttiType)
  private
    function GetInstanceType: TRttiInstanceType;
    function GetMetaclassType: TClass;
  public
    property InstanceType: TRttiInstanceType read GetInstanceType;
    property MetaclassType: TClass read GetMetaclassType;
  end;

  // A procedural type (tkProcedure, with the anonymous method types) and a method reference type
  // (tkMethod).
  TRttiProcedureType = class(TRttiType)
  private
    function GetReturnType: TRttiType;
  public
    function GetParameters: TArray<TRttiParameter>;
    property ReturnType: TRttiType read GetReturnType;
  end;

  TRttiMethodType = class(TRttiProcedureType)
  end;

  TRttiMember = class(TRttiNamedObject)
  private
    FTable: Integer;
    FIndex: Integer;
    FParent: PTypeInfo;
    function GetParent: TRttiType;
    function GetVisibility: TMemberVisibility;
  public
    // The type declaring the member.
    property Parent: TRttiType read GetParent;
    property Visibility: TMemberVisibility read GetVisibility;
  end;

  TRttiField = class(TRttiMember)
  private
    function GetFieldType: TRttiType;
  public
    function GetValue(Instance: TObject): TValue; overload;
    function GetValue(const Instance: TValue): TValue; overload;
    procedure SetValue(Instance: TObject; const AValue: TValue); overload;
    procedure SetValue(var Instance: TValue; const AValue: TValue); overload;
    property FieldType: TRttiType read GetFieldType;
  end;

  TRttiProperty = class(TRttiMember)
  private
    function GetPropertyType: TRttiType;
    function GetIsReadable: Boolean;
    function GetIsWritable: Boolean;
    function CallAccessor(Which: Integer; const V: TValue; const Args: TArray<TValue>): TValue;
  public
    function GetValue(Instance: TObject): TValue; overload;
    function GetValue(const Instance: TValue): TValue; overload;
    procedure SetValue(Instance: TObject; const AValue: TValue); overload;
    procedure SetValue(var Instance: TValue; const AValue: TValue); overload;
    property PropertyType: TRttiType read GetPropertyType;
    property IsReadable: Boolean read GetIsReadable;
    property IsWritable: Boolean read GetIsWritable;
  end;

  TRttiMethod = class(TRttiMember)
  private
    function GetMethodKind: TMethodKind;
    function GetReturnType: TRttiType;
    function GetIsConstructor: Boolean;
    function GetIsDestructor: Boolean;
    function GetIsClassMethod: Boolean;
    function GetIsStatic: Boolean;
  public
    function GetParameters: TArray<TRttiParameter>;
    // Calls the method (docs/spec/rtti.md 5.7): on an instance, a class reference (a class
    // method, a static method, a constructor) or what a TValue holds (also a record). The var
    // and out arguments are written back into Args.
    function Invoke(Instance: TObject; const Args: array of TValue): TValue; overload;
    function Invoke(Instance: TClass; const Args: array of TValue): TValue; overload;
    function Invoke(const Instance: TValue; const Args: array of TValue): TValue; overload;
    property MethodKind: TMethodKind read GetMethodKind;
    property ReturnType: TRttiType read GetReturnType;
    property IsConstructor: Boolean read GetIsConstructor;
    property IsDestructor: Boolean read GetIsDestructor;
    property IsClassMethod: Boolean read GetIsClassMethod;
    property IsStatic: Boolean read GetIsStatic;
  end;

  TRttiParameter = class(TRttiNamedObject)
  private
    FIndex: Integer;
    function GetParamType: TRttiType;
    function GetFlags: TParamFlags;
  public
    // An open array parameter has the dynamic array type of its elements.
    property ParamType: TRttiType read GetParamType;
    property Flags: TParamFlags read GetFlags;
  end;

  TRttiContext = record
  public
    // Nothing to make or free: kept so that code written for Delphi compiles.
    class function Create: TRttiContext; static;
    procedure Free;
    function GetType(ATypeInfo: PTypeInfo): TRttiType; overload;
    function GetType(AClass: TClass): TRttiType; overload;
    // The type of the fully qualified name (2.4; ignoring the case of ASCII letters), nil if
    // none.
    function FindType(const AQualifiedName: string): TRttiType;
    // Every type with type information, by fully qualified name (1.3).
    function GetTypes: TArray<TRttiType>;
  end;

implementation

// The fields of a type entry (RttiTypeField; docs/spec/bytecode.md 2.24).
const
  FieldKind = 0;
  FieldName = 2;
  FieldFullName = 3;
  FieldShape = 5;
  FieldMin = 6;
  FieldMax = 7;
  FieldBase = 8;
  FieldValueCount = 9;
  FieldValueOrd = 10;
  FieldValueName = 11;
  FieldFloatType = 12;
  FieldElem = 14;
  FieldElemCount = 16;
  FieldParent = 18;
  FieldClass = 20;
  FieldFieldsFirst = 21;
  FieldFieldsCount = 22;
  FieldPropsFirst = 23;
  FieldPropsCount = 24;
  FieldMethodsFirst = 25;
  FieldMethodsCount = 26;
  FieldAttrsFirst = 27;
  FieldAttrsCount = 28;
  FieldParamsFirst = 29;
  FieldParamsCount = 30;
  FieldResult = 31;
  FieldGenericDef = 32;
  FieldGenericArgCount = 33;
  FieldGenericArg = 34;

// The member tables and the fields of their entries (RttiMemberField).
const
  TableFields = 0;
  TableProps = 1;
  TableMethods = 2;
  TableParams = 3;
  MemberName = 0;
  MemberType = 1;
  MemberVisibility = 2;
  PropFlags = 3;
  MethodKindField = 1;
  MethodFlags = 3;
  MethodParamsFirst = 4;
  MethodParamsCount = 5;
  MethodResult = 6;
  ParamFlags = 2;
  FieldAttrsField = 5;
  PropReadKind = 4;
  PropWriteKind = 8;
  PropAttrsField = 12;
  MethodAttrsField = 10;
  ParamAttrsField = 3;

// The kinds of a property's accessor (docs/spec/bytecode.md 3.2): a method from TargetFunc on.
const
  TargetFunc = 2;

// The functions of the type information section called through (docs/spec/rtti.md 7.3, 7.4):
// a bridge calling a method through its method value, the function making an attribute.
type
  TRttiBridge = function(const Target: TProc; const Instance: TValue;
    const Args: TArray<TValue>): TValue;
  TRttiMake = function: TCustomAttribute;

// The flags of a property entry and of a method entry (docs/spec/bytecode.md 3.2).
const
  PropReadable = 1;
  PropWritable = 2;
  MethodStatic = 1;
  // The TParamFlags bits of pfVar and pfOut.
  ParamWritten = 1 or 32;

// The shapes of an ordinal type entry: the integer widths ShortInt (0) to UInt64 (7), Char,
// the enumerations.
const
  ShapeChar = 8;
  ShapeEnum = 9;

var
  // The objects made so far, by type information number and by entry index.
  GTypes: TArray<TRttiType>;
  GFields: TArray<TRttiField>;
  GProps: TArray<TRttiProperty>;
  GMethods: TArray<TRttiMethod>;
  GParams: TArray<TRttiParameter>;
  // The attributes' instances made so far, by the index of their entry (5.8).
  GAttrs: TArray<TCustomAttribute>;

// Whether two names are the same, ignoring the case of ASCII letters (as identifiers are).
function SameName(const A, B: string): Boolean;
begin
  if Length(A) <> Length(B) then
    Exit(False);
  for var I := Low(A) to High(A) do
  begin
    var X := A[I];
    var Y := B[I];
    if (X >= 'A') and (X <= 'Z') then
      X := Char(Ord(X) + 32);
    if (Y >= 'A') and (Y <= 'Z') then
      Y := Char(Ord(Y) + 32);
    if X <> Y then
      Exit(False);
  end;
  Result := True;
end;

// Room for the index I in an array of objects made on first use (doubled as it grows).
function Room(Count, I: Integer): Integer;
begin
  Result := Count;
  if Result < 16 then
    Result := 16;
  while Result <= I do
    Result := Result * 2;
end;

function TypeField(P: PTypeInfo; Field: Integer): Int64;
begin
  Result := RttiTypeField(P, Field, 0);
end;

// The type information of a type field holding a type information number.
function TypeOfField(P: PTypeInfo; Field, Index: Integer): PTypeInfo;
begin
  Result := TypeInfoOfNumber(Integer(RttiTypeField(P, Field, Index)));
end;

// A name of the name table ('' for none, -1).
function NameAt(Index: Int64): string;
begin
  if Index < 0 then
    Result := ''
  else
    Result := RttiName(Integer(Index));
end;

function NewType(P: PTypeInfo): TRttiType;
begin
  case TTypeKind(TypeField(P, FieldKind)) of
    TTypeKind.tkClass:
      Result := TRttiInstanceType.Create;
    TTypeKind.tkRecord:
      Result := TRttiRecordType.Create;
    TTypeKind.tkInteger, TTypeKind.tkInt64, TTypeKind.tkWChar:
      Result := TRttiOrdinalType.Create;
    TTypeKind.tkEnumeration:
      Result := TRttiEnumerationType.Create;
    TTypeKind.tkFloat:
      Result := TRttiFloatType.Create;
    TTypeKind.tkUString, TTypeKind.tkLString:
      Result := TRttiStringType.Create;
    TTypeKind.tkSet:
      Result := TRttiSetType.Create;
    TTypeKind.tkArray:
      Result := TRttiArrayType.Create;
    TTypeKind.tkDynArray:
      Result := TRttiDynamicArrayType.Create;
    TTypeKind.tkInterface:
      Result := TRttiInterfaceType.Create;
    TTypeKind.tkClassRef:
      Result := TRttiClassRefType.Create;
    TTypeKind.tkProcedure:
      Result := TRttiProcedureType.Create;
    TTypeKind.tkMethod:
      Result := TRttiMethodType.Create;
  else
    Result := TRttiType.Create;
  end;
  Result.FHandle := P;
  Result.FName := NameAt(TypeField(P, FieldName));
end;

// The one object of the type P in this isolate (nil for nil).
function TypeOf(P: PTypeInfo): TRttiType;
begin
  if P = nil then
    Exit(nil);
  var N := TypeInfoNumber(P);
  if N >= Length(GTypes) then
    SetLength(GTypes, Room(Length(GTypes), N));
  Result := GTypes[N];
  if Result = nil then
  begin
    Result := NewType(P);
    GTypes[N] := Result;
  end;
end;

function FieldAt(I: Integer; Parent: PTypeInfo): TRttiField;
begin
  if I >= Length(GFields) then
    SetLength(GFields, Room(Length(GFields), I));
  Result := GFields[I];
  if Result = nil then
  begin
    Result := TRttiField.Create;
    Result.FTable := TableFields;
    Result.FIndex := I;
    Result.FParent := Parent;
    Result.FName := NameAt(RttiMemberField(TableFields, I, MemberName));
    GFields[I] := Result;
  end;
end;

function PropAt(I: Integer; Parent: PTypeInfo): TRttiProperty;
begin
  if I >= Length(GProps) then
    SetLength(GProps, Room(Length(GProps), I));
  Result := GProps[I];
  if Result = nil then
  begin
    Result := TRttiProperty.Create;
    Result.FTable := TableProps;
    Result.FIndex := I;
    Result.FParent := Parent;
    Result.FName := NameAt(RttiMemberField(TableProps, I, MemberName));
    GProps[I] := Result;
  end;
end;

function MethodAt(I: Integer; Parent: PTypeInfo): TRttiMethod;
begin
  if I >= Length(GMethods) then
    SetLength(GMethods, Room(Length(GMethods), I));
  Result := GMethods[I];
  if Result = nil then
  begin
    Result := TRttiMethod.Create;
    Result.FTable := TableMethods;
    Result.FIndex := I;
    Result.FParent := Parent;
    Result.FName := NameAt(RttiMemberField(TableMethods, I, MemberName));
    GMethods[I] := Result;
  end;
end;

function ParamAt(I: Integer): TRttiParameter;
begin
  if I >= Length(GParams) then
    SetLength(GParams, Room(Length(GParams), I));
  Result := GParams[I];
  if Result = nil then
  begin
    Result := TRttiParameter.Create;
    Result.FIndex := I;
    Result.FName := NameAt(RttiMemberField(TableParams, I, MemberName));
    GParams[I] := Result;
  end;
end;

// The parameters First .. First + Count - 1 of the parameter table.
function ParamsAt(First, Count: Int64): TArray<TRttiParameter>;
begin
  SetLength(Result, Integer(Count));
  for var I := 0 to High(Result) do
    Result[I] := ParamAt(Integer(First) + I);
end;

{ TRttiType }

function TRttiType.GetTypeKind: TTypeKind;
begin
  Result := TTypeKind(TypeField(FHandle, FieldKind));
end;

function TRttiType.GetQualifiedName: string;
begin
  Result := NameAt(TypeField(FHandle, FieldFullName));
end;

function TRttiType.GetBaseType: TRttiType;
begin
  case GetTypeKind of
    TTypeKind.tkClass, TTypeKind.tkInterface:
      Result := TypeOf(TypeOfField(FHandle, FieldParent, 0));
  else
    Result := nil;
  end;
end;

function TRttiType.NotA(const What: string): EInvalidCast;
begin
  Result := EInvalidCast.Create('`' + Name + '` is not ' + What);
end;

function TRttiType.GetIsInstance: Boolean;
begin
  Result := Self is TRttiInstanceType;
end;

function TRttiType.GetAsInstance: TRttiInstanceType;
begin
  if not (Self is TRttiInstanceType) then
    raise NotA('a class type');
  Result := TRttiInstanceType(Self);
end;

function TRttiType.GetIsRecord: Boolean;
begin
  Result := Self is TRttiRecordType;
end;

function TRttiType.GetAsRecord: TRttiRecordType;
begin
  if not (Self is TRttiRecordType) then
    raise NotA('a record type');
  Result := TRttiRecordType(Self);
end;

function TRttiType.GetIsOrdinal: Boolean;
begin
  Result := Self is TRttiOrdinalType;
end;

function TRttiType.GetAsOrdinal: TRttiOrdinalType;
begin
  if not (Self is TRttiOrdinalType) then
    raise NotA('an ordinal type');
  Result := TRttiOrdinalType(Self);
end;

function TRttiType.GetIsSet: Boolean;
begin
  Result := Self is TRttiSetType;
end;

function TRttiType.GetAsSet: TRttiSetType;
begin
  if not (Self is TRttiSetType) then
    raise NotA('a set type');
  Result := TRttiSetType(Self);
end;

// The section names the definition with its type parameters (`….TList<T>`): the name is
// without the last of them.
function TRttiType.GetGenericDefinitionName: string;
begin
  Result := NameAt(TypeField(FHandle, FieldGenericDef));
  if (Result = '') or (Result[High(Result)] <> '>') then
    Exit;
  var Depth := 0;
  for var I := High(Result) downto Low(Result) do
  begin
    if Result[I] = '>' then
      Inc(Depth)
    else if Result[I] = '<' then
    begin
      Dec(Depth);
      if Depth = 0 then
        Exit(Copy(Result, Low(Result), I - Low(Result)));
    end;
  end;
end;

function TRttiType.GetGenericArguments: TArray<TRttiType>;
begin
  SetLength(Result, Integer(TypeField(FHandle, FieldGenericArgCount)));
  for var I := 0 to High(Result) do
    Result[I] := TypeOf(TypeOfField(FHandle, FieldGenericArg, I));
end;

function TRttiType.ToString: string;
begin
  Result := Name;
end;

// The members this type declares, then those of its ancestors (5.5).

function TRttiType.GetDeclaredFields: TArray<TRttiField>;
begin
  var First := TypeField(FHandle, FieldFieldsFirst);
  SetLength(Result, Integer(TypeField(FHandle, FieldFieldsCount)));
  for var I := 0 to High(Result) do
    Result[I] := FieldAt(Integer(First + I), FHandle);
end;

function TRttiType.GetFields: TArray<TRttiField>;
begin
  Result := GetDeclaredFields;
  var T := GetBaseType;
  while T <> nil do
  begin
    Result := Result + T.GetDeclaredFields;
    T := T.GetBaseType;
  end;
end;

function TRttiType.GetField(const AName: string): TRttiField;
begin
  for var F in GetFields do
    if SameName(F.Name, AName) then
      Exit(F);
  Result := nil;
end;

function TRttiType.GetDeclaredProperties: TArray<TRttiProperty>;
begin
  var First := TypeField(FHandle, FieldPropsFirst);
  SetLength(Result, Integer(TypeField(FHandle, FieldPropsCount)));
  for var I := 0 to High(Result) do
    Result[I] := PropAt(Integer(First + I), FHandle);
end;

function TRttiType.GetProperties: TArray<TRttiProperty>;
begin
  Result := GetDeclaredProperties;
  var T := GetBaseType;
  while T <> nil do
  begin
    Result := Result + T.GetDeclaredProperties;
    T := T.GetBaseType;
  end;
end;

function TRttiType.GetProperty(const AName: string): TRttiProperty;
begin
  for var P in GetProperties do
    if SameName(P.Name, AName) then
      Exit(P);
  Result := nil;
end;

function TRttiType.GetDeclaredMethods: TArray<TRttiMethod>;
begin
  var First := TypeField(FHandle, FieldMethodsFirst);
  SetLength(Result, Integer(TypeField(FHandle, FieldMethodsCount)));
  for var I := 0 to High(Result) do
    Result[I] := MethodAt(Integer(First + I), FHandle);
end;

function TRttiType.GetMethods: TArray<TRttiMethod>;
begin
  Result := GetDeclaredMethods;
  var T := GetBaseType;
  while T <> nil do
  begin
    Result := Result + T.GetDeclaredMethods;
    T := T.GetBaseType;
  end;
end;

function TRttiType.GetMethods(const AName: string): TArray<TRttiMethod>;
begin
  Result := nil;
  for var M in GetMethods do
    if SameName(M.Name, AName) then
      Result := Result + [M];
end;

function TRttiType.GetMethod(const AName: string): TRttiMethod;
begin
  for var M in GetMethods do
    if SameName(M.Name, AName) then
      Exit(M);
  Result := nil;
end;

{ TRttiInstanceType }

function TRttiInstanceType.GetMetaclassType: TClass;
begin
  Result := RttiClassOf(FHandle);
end;

function TRttiInstanceType.GetInstanceBaseType: TRttiInstanceType;
begin
  Result := TRttiInstanceType(GetBaseType);
end;

{ TRttiOrdinalType }

function TRttiOrdinalType.GetOrdType: TOrdType;
begin
  case TypeField(FHandle, FieldShape) of
    0: Result := TOrdType.otSByte;
    1: Result := TOrdType.otUByte;
    2: Result := TOrdType.otSWord;
    3, ShapeChar: Result := TOrdType.otUWord;
    5, 7: Result := TOrdType.otULong;
    ShapeEnum:
      // An enumeration takes the smallest unsigned width that holds its values
      if (GetMinValue >= 0) and (GetMaxValue <= 255) then
        Result := TOrdType.otUByte
      else if (GetMinValue >= 0) and (GetMaxValue <= 65535) then
        Result := TOrdType.otUWord
      else
        Result := TOrdType.otSLong;
  else
    Result := TOrdType.otSLong;
  end;
end;

function TRttiOrdinalType.GetMinValue: Int64;
begin
  Result := TypeField(FHandle, FieldMin);
end;

function TRttiOrdinalType.GetMaxValue: Int64;
begin
  Result := TypeField(FHandle, FieldMax);
end;

{ TRttiEnumerationType }

// The entry holding the values of the enumeration P: P, or the enumeration P is a subrange of.
function EnumValues(P: PTypeInfo): PTypeInfo;
begin
  Result := P;
  while (TypeField(Result, FieldValueCount) = 0) and (TypeField(Result, FieldBase) <> 0) do
    Result := TypeOfField(Result, FieldBase, 0);
end;

function TRttiEnumerationType.GetUnderlyingType: TRttiType;
begin
  if TypeField(FHandle, FieldBase) <> 0 then
    Result := TypeOf(TypeOfField(FHandle, FieldBase, 0))
  else
    Result := Self;
end;

function TRttiEnumerationType.GetNames: TArray<string>;
begin
  var P := EnumValues(FHandle);
  var Lo := GetMinValue;
  var Hi := GetMaxValue;
  Result := nil;
  for var I := 0 to RttiTypeField(P, FieldValueCount, 0) - 1 do
  begin
    var Value := RttiTypeField(P, FieldValueOrd, I);
    if (Value >= Lo) and (Value <= Hi) then
      Result := Result + [NameAt(RttiTypeField(P, FieldValueName, I))];
  end;
end;

class function TRttiEnumerationType.GetValue<T>(const AName: string): T;
begin
  var Info := System.TypeInfo(T);
  var P := EnumValues(Info);
  for var I := 0 to RttiTypeField(P, FieldValueCount, 0) - 1 do
    if SameName(NameAt(RttiTypeField(P, FieldValueName, I)), AName) then
      Exit(TValue.FromOrdinal(Info, RttiTypeField(P, FieldValueOrd, I)).AsType<T>);
  raise EArgumentException.Create('`' + AName + '` is not a value of `' +
    NameAt(TypeField(Info, FieldName)) + '`');
end;

class function TRttiEnumerationType.GetName<T>(AValue: T): string;
begin
  var Value := TValue.From<T>(AValue).AsOrdinal;
  var P := EnumValues(System.TypeInfo(T));
  for var I := 0 to RttiTypeField(P, FieldValueCount, 0) - 1 do
    if RttiTypeField(P, FieldValueOrd, I) = Value then
      Exit(NameAt(RttiTypeField(P, FieldValueName, I)));
  Result := Value.ToString;
end;

{ TRttiFloatType }

function TRttiFloatType.GetFloatType: TFloatType;
begin
  Result := TFloatType(TypeField(FHandle, FieldFloatType));
end;

{ TRttiSetType }

function TRttiSetType.GetElementType: TRttiType;
begin
  Result := TypeOf(TypeOfField(FHandle, FieldBase, 0));
end;

{ TRttiArrayType }

function TRttiArrayType.GetElementType: TRttiType;
begin
  Result := TypeOf(TypeOfField(FHandle, FieldElem, 0));
end;

function TRttiArrayType.GetTotalElementCount: Integer;
begin
  Result := Integer(TypeField(FHandle, FieldElemCount));
end;

function TRttiArrayType.GetDimensionCount: Integer;
begin
  Result := 1;
end;

{ TRttiDynamicArrayType }

function TRttiDynamicArrayType.GetElementType: TRttiType;
begin
  Result := TypeOf(TypeOfField(FHandle, FieldElem, 0));
end;

{ TRttiInterfaceType }

function TRttiInterfaceType.GetInterfaceBaseType: TRttiInterfaceType;
begin
  Result := TRttiInterfaceType(GetBaseType);
end;

{ TRttiClassRefType }

function TRttiClassRefType.GetInstanceType: TRttiInstanceType;
begin
  Result := TRttiInstanceType(TypeOf(TypeOfField(FHandle, FieldClass, 0)));
end;

function TRttiClassRefType.GetMetaclassType: TClass;
begin
  Result := RttiClassOf(TypeOfField(FHandle, FieldClass, 0));
end;

{ TRttiProcedureType }

function TRttiProcedureType.GetReturnType: TRttiType;
begin
  Result := TypeOf(TypeOfField(FHandle, FieldResult, 0));
end;

function TRttiProcedureType.GetParameters: TArray<TRttiParameter>;
begin
  Result := ParamsAt(TypeField(FHandle, FieldParamsFirst), TypeField(FHandle, FieldParamsCount));
end;

{ TRttiObject }

procedure TRttiObject.AttrRange(out First, Count: Integer);
var
  F: Integer;
begin
  if Self is TRttiType then
  begin
    First := Integer(TypeField(TRttiType(Self).FHandle, FieldAttrsFirst));
    Count := Integer(TypeField(TRttiType(Self).FHandle, FieldAttrsCount));
  end
  else if Self is TRttiMember then
  begin
    var M := TRttiMember(Self);
    case M.FTable of
      TableFields:
        F := FieldAttrsField;
      TableProps:
        F := PropAttrsField;
    else
      F := MethodAttrsField;
    end;
    First := Integer(RttiMemberField(M.FTable, M.FIndex, F));
    Count := Integer(RttiMemberField(M.FTable, M.FIndex, F + 1));
  end
  else if Self is TRttiParameter then
  begin
    var P := TRttiParameter(Self);
    First := Integer(RttiMemberField(TableParams, P.FIndex, ParamAttrsField));
    Count := Integer(RttiMemberField(TableParams, P.FIndex, ParamAttrsField + 1));
  end
  else
  begin
    First := 0;
    Count := 0;
  end;
end;

// The one instance of the attribute I in this isolate, made when first asked for. If its
// constructor raises, none is kept: the next question makes it again (docs/spec/rtti.md 5.8).
class function TRttiObject.AttrAt(I: Integer): TCustomAttribute;
begin
  if I >= Length(GAttrs) then
    SetLength(GAttrs, Room(Length(GAttrs), I));
  Result := GAttrs[I];
  if Result = nil then
  begin
    var Make := RttiAttrFunc(TRttiMake, I);
    if not Assigned(Make) then
      Exit(nil);
    Result := Make();
    GAttrs[I] := Result;
  end;
end;

function TRttiObject.GetAttributes: TArray<TCustomAttribute>;
var
  First, Count: Integer;
begin
  AttrRange(First, Count);
  SetLength(Result, Count);
  for var I := 0 to Count - 1 do
    Result[I] := AttrAt(First + I);
end;

function TRttiObject.GetAttribute<T>: T;
begin
  for var A in GetAttributes do
    if A is T then
      Exit(A as T);
  Result := nil;
end;

function TRttiObject.HasAttribute<T>: Boolean;
begin
  Result := GetAttribute<T> <> nil;
end;

{ TRttiMember }

function TRttiMember.GetParent: TRttiType;
begin
  Result := TypeOf(FParent);
end;

function TRttiMember.GetVisibility: TMemberVisibility;
begin
  Result := TMemberVisibility(RttiMemberField(FTable, FIndex, MemberVisibility));
end;

{ TRttiField }

function TRttiField.GetFieldType: TRttiType;
begin
  Result := TypeOf(TypeInfoOfNumber(Integer(RttiMemberField(TableFields, FIndex, MemberType))));
end;

function TRttiField.GetValue(Instance: TObject): TValue;
begin
  var V: TValue := Instance;
  Result := RttiFieldGet(TableFields, FIndex, V);
end;

function TRttiField.GetValue(const Instance: TValue): TValue;
begin
  Result := RttiFieldGet(TableFields, FIndex, Instance);
end;

procedure TRttiField.SetValue(Instance: TObject; const AValue: TValue);
begin
  var V: TValue := Instance;
  RttiFieldSet(TableFields, FIndex, V, AValue);
end;

procedure TRttiField.SetValue(var Instance: TValue; const AValue: TValue);
begin
  Instance := RttiFieldSet(TableFields, FIndex, Instance, AValue);
end;

{ TRttiProperty }

function TRttiProperty.GetPropertyType: TRttiType;
begin
  Result := TypeOf(TypeInfoOfNumber(Integer(RttiMemberField(TableProps, FIndex, MemberType))));
end;

function TRttiProperty.GetIsReadable: Boolean;
begin
  Result := RttiMemberField(TableProps, FIndex, PropFlags) and PropReadable <> 0;
end;

function TRttiProperty.GetIsWritable: Boolean;
begin
  Result := RttiMemberField(TableProps, FIndex, PropFlags) and PropWritable <> 0;
end;

// Calls the read (Which 0) or write (1) method of the property through its bridge
// (docs/spec/rtti.md 5.6): the instance, record or class reference V holds is checked as for
// Invoke.
function TRttiProperty.CallAccessor(Which: Integer; const V: TValue;
  const Args: TArray<TValue>): TValue;
begin
  var Target := RttiPropRef(FIndex, Which, V);
  var Bridge := RttiBridge(TRttiBridge, TableProps, FIndex, Which);
  if not Assigned(Bridge) then
    raise EInvalidOperation.Create('property `' + Name + '` cannot be read or written by its ' +
      'method');
  Result := Bridge(Target, V, Args);
end;

function TRttiProperty.GetValue(Instance: TObject): TValue;
begin
  var V: TValue := Instance;
  Result := GetValue(V);
end;

function TRttiProperty.GetValue(const Instance: TValue): TValue;
begin
  if RttiMemberField(TableProps, FIndex, PropReadKind) >= TargetFunc then
    Result := CallAccessor(0, Instance, nil)
  else
    Result := RttiFieldGet(TableProps, FIndex, Instance);
end;

procedure TRttiProperty.SetValue(Instance: TObject; const AValue: TValue);
begin
  var V: TValue := Instance;
  SetValue(V, AValue);
end;

procedure TRttiProperty.SetValue(var Instance: TValue; const AValue: TValue);
begin
  if RttiMemberField(TableProps, FIndex, PropWriteKind) >= TargetFunc then
    CallAccessor(1, Instance, [AValue])
  else
    Instance := RttiFieldSet(TableProps, FIndex, Instance, AValue);
end;

{ TRttiMethod }

function TRttiMethod.GetMethodKind: TMethodKind;
begin
  Result := TMethodKind(RttiMemberField(TableMethods, FIndex, MethodKindField));
end;

function TRttiMethod.GetReturnType: TRttiType;
begin
  Result := TypeOf(TypeInfoOfNumber(Integer(RttiMemberField(TableMethods, FIndex, MethodResult))));
end;

function TRttiMethod.GetIsConstructor: Boolean;
begin
  Result := GetMethodKind = TMethodKind.mkConstructor;
end;

function TRttiMethod.GetIsDestructor: Boolean;
begin
  Result := GetMethodKind = TMethodKind.mkDestructor;
end;

function TRttiMethod.GetIsClassMethod: Boolean;
begin
  Result := GetMethodKind in [TMethodKind.mkClassProcedure, TMethodKind.mkClassFunction];
end;

function TRttiMethod.GetIsStatic: Boolean;
begin
  Result := RttiMemberField(TableMethods, FIndex, MethodFlags) and MethodStatic <> 0;
end;

function TRttiMethod.GetParameters: TArray<TRttiParameter>;
begin
  Result := ParamsAt(RttiMemberField(TableMethods, FIndex, MethodParamsFirst),
    RttiMemberField(TableMethods, FIndex, MethodParamsCount));
end;

function TRttiMethod.Invoke(Instance: TObject; const Args: array of TValue): TValue;
begin
  var V: TValue := Instance;
  Result := Invoke(V, Args);
end;

function TRttiMethod.Invoke(Instance: TClass; const Args: array of TValue): TValue;
begin
  var V := TValue.From<TClass>(Instance);
  Result := Invoke(V, Args);
end;

function TRttiMethod.Invoke(const Instance: TValue; const Args: array of TValue): TValue;
begin
  var First := Integer(RttiMemberField(TableMethods, FIndex, MethodParamsFirst));
  var Count := Integer(RttiMemberField(TableMethods, FIndex, MethodParamsCount));
  if Length(Args) <> Count then
    raise EArgumentException.Create('`' + Name + '` expects ' + Count.ToString +
      ' arguments, but ' + Length(Args).ToString + ' were given');
  if GetIsDestructor then
  begin
    // The instance checked as for any method of an instance (the method value is not used),
    // then the destruction sequence, as Free (a reference counted class refuses it)
    if not Assigned(RttiMethodRef(FIndex, Instance)) then
      raise EInvalidOperation.Create('method `' + Name + '` cannot be invoked');
    Instance.AsObject.Free;
    Exit(Default(TValue));
  end;
  var Target := RttiMethodRef(FIndex, Instance);
  var Bridge := RttiBridge(TRttiBridge, TableMethods, FIndex, 0);
  if not Assigned(Bridge) then
    raise EInvalidOperation.Create('method `' + Name + '` cannot be invoked');
  var A: TArray<TValue>;
  SetLength(A, Count);
  for var I := 0 to Count - 1 do
    A[I] := Args[I];
  Result := Bridge(Target, Instance, A);
  // The var and out arguments the bridge wrote into A go back to the caller's Args
  for var I := 0 to Count - 1 do
    if RttiMemberField(TableParams, First + I, ParamFlags) and ParamWritten <> 0 then
      RttiArgSet(Args, I, A[I]);
end;

{ TRttiParameter }

function TRttiParameter.GetParamType: TRttiType;
begin
  Result := TypeOf(TypeInfoOfNumber(Integer(RttiMemberField(TableParams, FIndex, MemberType))));
end;

function TRttiParameter.GetFlags: TParamFlags;
begin
  // Bit I of the entry's flags is the flag of ordinal I
  var Bits := RttiMemberField(TableParams, FIndex, ParamFlags);
  Result := [];
  for var F := Low(TParamFlag) to High(TParamFlag) do
    if (Bits shr Ord(F)) and 1 <> 0 then
      Include(Result, F);
end;

{ TRttiContext }

class function TRttiContext.Create: TRttiContext;
begin
  Result := Default(TRttiContext);
end;

procedure TRttiContext.Free;
begin
end;

function TRttiContext.GetType(ATypeInfo: PTypeInfo): TRttiType;
begin
  Result := TypeOf(ATypeInfo);
end;

function TRttiContext.GetType(AClass: TClass): TRttiType;
begin
  Result := TypeOf(RttiClassType(AClass));
end;

function TRttiContext.FindType(const AQualifiedName: string): TRttiType;
begin
  Result := TypeOf(RttiFindType(AQualifiedName));
end;

function TRttiContext.GetTypes: TArray<TRttiType>;
begin
  SetLength(Result, RttiTypeCount);
  for var I := 0 to High(Result) do
    Result[I] := TypeOf(TypeInfoOfNumber(I + 1));
end;

end.
