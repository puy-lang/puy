unit System.Generics.Defaults;

// The comparers (docs/spec/collections.md 10.1, docs/spec/equality.md chapter 6). Written from
// the specification only, not from any other implementation.
//
// A comparer is a reference-counted object held through its interface; it is never freed by
// hand. Default calls KeyCompare / KeyEquals / KeyHash; Construct keeps the routine it is given
// (counted like any other value of a `reference to` type).

interface

type
  // An order: negative when Left comes first, 0 when neither does, positive otherwise.
  TComparison<T> = reference to function(const Left, Right: T): Integer;

  // An equality; must agree with the THasher<T> it is paired with (equality.md 1.2).
  TEqualityComparison<T> = reference to function(const Left, Right: T): Boolean;

  // A hash that gives equal values the same result.
  THasher<T> = reference to function(const Value: T): Int64;

  IComparer<T> = interface
    function Compare(const Left, Right: T): Integer;
  end;

  IEqualityComparer<T> = interface
    function Equals(const Left, Right: T): Boolean;
    function GetHashCode(const Value: T): Int64;
  end;

  TComparer<T> = class abstract(TRefObject, IComparer<T>)
  public
    // The default order of T (KeyCompare). Asking for it never fails; comparing values of a type
    // with no default order raises EInvalidOperation.
    class function Default: IComparer<T>;
    // A comparer that calls Comparison.
    class function Construct(const Comparison: TComparison<T>): IComparer<T>;
    function Compare(const Left, Right: T): Integer; virtual; abstract;
  end;

  TEqualityComparer<T> = class abstract(TRefObject, IEqualityComparer<T>)
  public
    // The key equality and key hash of T (KeyEquals / KeyHash).
    class function Default: IEqualityComparer<T>;
    // A comparer that calls EqualityComparison and Hasher.
    class function Construct(const EqualityComparison: TEqualityComparison<T>;
      const Hasher: THasher<T>): IEqualityComparer<T>;
    // These overload TObject.Equals(Obj) and TObject.GetHashCode with other parameters.
    function Equals(const Left, Right: T): Boolean; reintroduce; overload; virtual; abstract;
    function GetHashCode(const Value: T): Int64; reintroduce; overload; virtual; abstract;
  end;

  TStringComparer = class
  public
    // Compares the UTF-16 code units (the same as TEqualityComparer<string>.Default).
    class function Ordinal: IEqualityComparer<string>;
    // Folds the ASCII letters only, like SameText.
    class function OrdinalIgnoreCase: IEqualityComparer<string>;
  end;

implementation

uses
  System.SysUtils;

type
  TDefaultComparer<T> = class(TComparer<T>)
  public
    function Compare(const Left, Right: T): Integer; override;
  end;

  TDelegatedComparer<T> = class(TComparer<T>)
  private
    FComparison: TComparison<T>;
  public
    constructor Create(const Comparison: TComparison<T>);
    function Compare(const Left, Right: T): Integer; override;
  end;

  TDefaultEqualityComparer<T> = class(TEqualityComparer<T>)
  public
    function Equals(const Left, Right: T): Boolean; override;
    function GetHashCode(const Value: T): Int64; override;
  end;

  TDelegatedEqualityComparer<T> = class(TEqualityComparer<T>)
  private
    FEquals: TEqualityComparison<T>;
    FHasher: THasher<T>;
  public
    constructor Create(const EqualityComparison: TEqualityComparison<T>;
      const Hasher: THasher<T>);
    function Equals(const Left, Right: T): Boolean; override;
    function GetHashCode(const Value: T): Int64; override;
  end;

  TOrdinalIgnoreCaseComparer = class(TEqualityComparer<string>)
  public
    function Equals(const Left, Right: string): Boolean; override;
    function GetHashCode(const Value: string): Int64; override;
  end;

// ===== TComparer<T> =====

class function TComparer<T>.Default: IComparer<T>;
begin
  Result := TDefaultComparer<T>.Create;
end;

class function TComparer<T>.Construct(const Comparison: TComparison<T>): IComparer<T>;
begin
  Result := TDelegatedComparer<T>.Create(Comparison);
end;

function TDefaultComparer<T>.Compare(const Left, Right: T): Integer;
begin
  Result := KeyCompare<T>(Left, Right);
end;

constructor TDelegatedComparer<T>.Create(const Comparison: TComparison<T>);
begin
  inherited Create;
  FComparison := Comparison;
end;

function TDelegatedComparer<T>.Compare(const Left, Right: T): Integer;
begin
  Result := FComparison(Left, Right);
end;

// ===== TEqualityComparer<T> =====

class function TEqualityComparer<T>.Default: IEqualityComparer<T>;
begin
  Result := TDefaultEqualityComparer<T>.Create;
end;

class function TEqualityComparer<T>.Construct(const EqualityComparison: TEqualityComparison<T>;
  const Hasher: THasher<T>): IEqualityComparer<T>;
begin
  Result := TDelegatedEqualityComparer<T>.Create(EqualityComparison, Hasher);
end;

function TDefaultEqualityComparer<T>.Equals(const Left, Right: T): Boolean;
begin
  Result := KeyEquals<T>(Left, Right);
end;

function TDefaultEqualityComparer<T>.GetHashCode(const Value: T): Int64;
begin
  Result := KeyHash<T>(Value);
end;

constructor TDelegatedEqualityComparer<T>.Create(
  const EqualityComparison: TEqualityComparison<T>; const Hasher: THasher<T>);
begin
  inherited Create;
  FEquals := EqualityComparison;
  FHasher := Hasher;
end;

function TDelegatedEqualityComparer<T>.Equals(const Left, Right: T): Boolean;
begin
  Result := FEquals(Left, Right);
end;

function TDelegatedEqualityComparer<T>.GetHashCode(const Value: T): Int64;
begin
  Result := FHasher(Value);
end;

// ===== TStringComparer =====

class function TStringComparer.Ordinal: IEqualityComparer<string>;
begin
  Result := TEqualityComparer<string>.Default;
end;

class function TStringComparer.OrdinalIgnoreCase: IEqualityComparer<string>;
begin
  Result := TOrdinalIgnoreCaseComparer.Create;
end;

function TOrdinalIgnoreCaseComparer.Equals(const Left, Right: string): Boolean;
begin
  Result := SameText(Left, Right);
end;

function TOrdinalIgnoreCaseComparer.GetHashCode(const Value: string): Int64;
begin
  // SameText folds the ASCII lower-case letters to upper case; hash the same folded form.
  Result := KeyHash<string>(UpperCase(Value));
end;

end.
