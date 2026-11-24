// ===== Integer type helpers (docs/spec/stdlib/System/Helpers.md) =====

type
  // The members of `Byte` besides the built-in ToString.
  TByteHelper = record helper for Byte
  public
    const MaxValue = High(Byte);
    const MinValue = Low(Byte);
    class function Parse(const S: string): Byte; static;
    class function TryParse(const S: string; out Value: Byte): Boolean; static;

    function ToBoolean: Boolean;
    function ToBytes: TArray<Byte>;
    function ToDouble: Double;
    function ToHexString: string; overload;
    function ToHexString(MinDigits: Integer): string; overload;
    function ToSingle: Single;
  end;

  // The members of `ShortInt` besides the built-in ToString.
  TShortIntHelper = record helper for ShortInt
  public
    const MaxValue = High(ShortInt);
    const MinValue = Low(ShortInt);
    class function Parse(const S: string): ShortInt; static;
    class function TryParse(const S: string; out Value: ShortInt): Boolean; static;

    function ToBoolean: Boolean;
    function ToBytes: TArray<Byte>;
    function ToDouble: Double;
    function ToHexString: string; overload;
    function ToHexString(MinDigits: Integer): string; overload;
    function ToSingle: Single;
  end;

  // The members of `Word` besides the built-in ToString.
  TWordHelper = record helper for Word
  public
    const MaxValue = High(Word);
    const MinValue = Low(Word);
    class function Parse(const S: string): Word; static;
    class function TryParse(const S: string; out Value: Word): Boolean; static;

    function ToBoolean: Boolean;
    function ToBytes: TArray<Byte>;
    function ToDouble: Double;
    function ToHexString: string; overload;
    function ToHexString(MinDigits: Integer): string; overload;
    function ToSingle: Single;
  end;

  // The members of `SmallInt` besides the built-in ToString.
  TSmallIntHelper = record helper for SmallInt
  public
    const MaxValue = High(SmallInt);
    const MinValue = Low(SmallInt);
    class function Parse(const S: string): SmallInt; static;
    class function TryParse(const S: string; out Value: SmallInt): Boolean; static;

    function ToBoolean: Boolean;
    function ToBytes: TArray<Byte>;
    function ToDouble: Double;
    function ToHexString: string; overload;
    function ToHexString(MinDigits: Integer): string; overload;
    function ToSingle: Single;
  end;

  // The members of `Cardinal` besides the built-in ToString.
  TCardinalHelper = record helper for Cardinal
  public
    const MaxValue = High(Cardinal);
    const MinValue = Low(Cardinal);
    class function Parse(const S: string): Cardinal; static;
    class function TryParse(const S: string; out Value: Cardinal): Boolean; static;

    function ToBoolean: Boolean;
    function ToBytes: TArray<Byte>;
    function ToDouble: Double;
    function ToHexString: string; overload;
    function ToHexString(MinDigits: Integer): string; overload;
    function ToSingle: Single;
  end;

  // The members of `Integer` besides the built-in ToString.
  TIntegerHelper = record helper for Integer
  public
    const MaxValue = High(Integer);
    const MinValue = Low(Integer);
    class function Parse(const S: string): Integer; static;
    class function TryParse(const S: string; out Value: Integer): Boolean; static;

    function ToBoolean: Boolean;
    function ToBytes: TArray<Byte>;
    function ToDouble: Double;
    function ToHexString: string; overload;
    function ToHexString(MinDigits: Integer): string; overload;
    function ToSingle: Single;
  end;

  // The members of `UInt64` besides the built-in ToString.
  TUInt64Helper = record helper for UInt64
  public
    const MaxValue = High(UInt64);
    const MinValue = Low(UInt64);
    class function Parse(const S: string): UInt64; static;
    class function TryParse(const S: string; out Value: UInt64): Boolean; static;

    function ToBoolean: Boolean;
    function ToBytes: TArray<Byte>;
    function ToDouble: Double;
    function ToHexString: string; overload;
    function ToHexString(MinDigits: Integer): string; overload;
    function ToSingle: Single;
  end;

  // The members of `Int64` besides the built-in ToString.
  TInt64Helper = record helper for Int64
  public
    const MaxValue = High(Int64);
    const MinValue = Low(Int64);
    class function Parse(const S: string): Int64; static;
    class function TryParse(const S: string; out Value: Int64): Boolean; static;

    function ToBoolean: Boolean;
    function ToBytes: TArray<Byte>;
    function ToDouble: Double;
    function ToHexString: string; overload;
    function ToHexString(MinDigits: Integer): string; overload;
    function ToSingle: Single;
  end;
