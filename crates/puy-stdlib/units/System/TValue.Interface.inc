// ===== TValue (docs/spec/rtti.md chapter 3) =====

type
  // A value of any type with the type it was made from (docs/spec/rtti.md 3.1): FType is the
  // type information number (0 for the empty value), FBits the value of a type not managed, FRef
  // the reference of a managed one. Every field 0 is the empty value. System.Rtti names the same
  // type. An `array of const` parameter is an open array of TValue (4.1).
  TValue = record
  private
    FType: Integer;
    FBits: Int64;
    FRef: TValueRef;
    class function GetEmpty: TValue; static;
    function GetIsEmpty: Boolean;
    function GetKind: TTypeKind;
    function GetTypeInfo: PTypeInfo;
  public
    class function From<T>(const AValue: T): TValue; static;
    class function FromOrdinal(ATypeInfo: PTypeInfo; AValue: Int64): TValue; static;
    function AsType<T>: T;
    function TryAsType<T>(out AResult: T): Boolean;
    function IsType<T>: Boolean;

    function IsObject: Boolean;
    function IsClass: Boolean;
    function IsOrdinal: Boolean;
    function IsArray: Boolean;

    function AsInteger: Integer;
    function AsInt64: Int64;
    function AsOrdinal: Int64;
    function AsBoolean: Boolean;
    function AsExtended: Double;
    function AsCurrency: Currency;
    function AsString: string;
    function AsObject: TObject;
    function AsClass: TClass;
    function AsInterface: IInterface;

    function GetArrayLength: Integer;
    function GetArrayElement(Index: Integer): TValue;
    procedure SetArrayElement(Index: Integer; const AValue: TValue);

    function ToString: string;

    class operator Implicit(const Value: string): TValue;
    class operator Implicit(Value: Integer): TValue;
    class operator Implicit(Value: Cardinal): TValue;
    class operator Implicit(Value: Int64): TValue;
    class operator Implicit(Value: UInt64): TValue;
    class operator Implicit(Value: Boolean): TValue;
    class operator Implicit(Value: Double): TValue;
    class operator Implicit(Value: Currency): TValue;
    class operator Implicit(Value: TObject): TValue;
    class operator Implicit(Value: TClass): TValue;

    property IsEmpty: Boolean read GetIsEmpty;
    property Kind: TTypeKind read GetKind;
    property TypeInfo: PTypeInfo read GetTypeInfo;
    class property Empty: TValue read GetEmpty;
  end;
