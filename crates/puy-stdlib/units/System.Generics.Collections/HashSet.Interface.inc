  // ===== THashSet<T> (chapter 8) =====
type
  // A set that keeps its items in insertion order (chapter 8). The items are the keys of a
  // dictionary the set owns, so the set shares the dictionary's hash table: the same order,
  // comparison, checks of changed items (7.4, 7.5) and messages, which name the set.
  THashSet<T> = class(TEnumerable<T>)
  private
    type
      TTable = TDictionary<T, Boolean>;
  public
    type
      // The items are the keys of the table, enumerated as the dictionary's keys are
      TEnumerator = TTable.TKeyEnumerator;
  private
    FTable: TTable;
    procedure Init(const AComparer: IEqualityComparer<T>);
    function GetCount: Integer;
    function GetCapacity: Integer;
    procedure SetCapacity(Value: Integer);
  public
    constructor Create; overload;
    constructor Create(const AComparer: IEqualityComparer<T>); overload;
    constructor Create(const Values: array of T); overload;
    constructor Create(const Source: TEnumerable<T>); overload;
    destructor Destroy; override;
    function Add(const Value: T): Boolean;
    function Remove(const Value: T): Boolean;
    function Contains(const Value: T): Boolean;
    procedure UnionWith(const Source: TEnumerable<T>); overload;
    procedure UnionWith(const Values: array of T); overload;
    procedure IntersectWith(const Source: TEnumerable<T>); overload;
    procedure IntersectWith(const Values: array of T); overload;
    procedure ExceptWith(const Source: TEnumerable<T>); overload;
    procedure ExceptWith(const Values: array of T); overload;
    procedure Clear;
    procedure TrimExcess;
    function ToArray: TArray<T>; override;
    function GetEnumerator: TEnumerator;
    property Count: Integer read GetCount;
    property Capacity: Integer read GetCapacity write SetCapacity;
  end;
