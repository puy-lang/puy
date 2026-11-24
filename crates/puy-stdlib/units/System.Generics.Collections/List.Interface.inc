  // ===== TList<T> (chapter 5) =====
type
  // A growable sequence read and written by index (chapter 5). The items live in FItems[0 ..
  // FCount - 1]; the slots after them hold Default(T), so Length(FItems) is the capacity.
  TList<T> = class(TEnumerable<T>)
  public
    type
      // Walks the list in index order without copying it; raises EInvalidOperation when the
      // list was changed since the enumerator was made (3.3, 3.4).
      TEnumerator = record
      private
        FList: TList<T>;
        FIndex: Integer;
        FVersion: Integer;
        function GetCurrent: T;
      public
        function MoveNext: Boolean;
        property Current: T read GetCurrent;
      end;
  private
    FItems: TArray<T>;
    FCount: Integer;
    // Bumped by every change of the structure (3.3); not by writing a value in place.
    FVersion: Integer;
    // The order of Sort and BinarySearch without a comparer (5.3); nil for the default order.
    FComparer: IComparer<T>;
    // Whether Notify is called (NOTIFY_*): learned when there is first something to tell (12.1).
    FNotify: Integer;
    function Notifies: Boolean;
    procedure NotifyItem(const Item: T; Action: TCollectionNotification);
    procedure CheckIndex(Index: Integer);
    procedure CheckInsertIndex(Index: Integer);
    procedure Grow(ACount: Integer);
    procedure InsertArray(Index: Integer; const Values: array of T);
    function GetItem(Index: Integer): T;
    procedure SetItem(Index: Integer; const Value: T);
    function GetRange(const Range: TIndexRange): TArray<T>;
    procedure SetCount(Value: Integer);
    function GetCapacity: Integer;
    procedure SetCapacity(Value: Integer);
    // Tells Notify that Items[From..] were removed, in order; when one call raises, the rest are
    // still told, and the exceptions overlap (9.1).
    procedure ItemsRemoved(const Items: array of T; From: Integer);
    // Removes the item at Index without telling Notify.
    procedure RemoveAt(Index: Integer);
    // Clear, and when the list is being freed (not KeepCapacity) also lets go of the array.
    procedure RemoveAll(KeepCapacity: Boolean);
  protected
    // Told cnAdding before Item enters the list (Add, Insert, AddRange, InsertRange,
    // Items[I] := X; raising refuses it, and the list is left as it was), and cnRemoved after
    // Item has left it (Delete, DeleteRange, Remove, Clear, a smaller Count, freeing the list,
    // the item Items[I] := X wrote over unless it is X itself), but not when it is taken out
    // (Extract, ExtractAt). Does nothing here, and is only called when a descendant overrides
    // it; an owning list frees the item (9.1, 9.4, 12.1).
    procedure Notify(const Item: T; Action: TCollectionNotification); virtual;
  public
    constructor Create; overload;
    constructor Create(const AComparer: IComparer<T>); overload;
    constructor Create(const Values: array of T); overload;
    constructor Create(const Source: TEnumerable<T>); overload;
    destructor Destroy; override;
    function Add(const Value: T): Integer;
    procedure AddRange(const Values: array of T); overload;
    procedure AddRange(const Source: TEnumerable<T>); overload;
    procedure Insert(Index: Integer; const Value: T);
    procedure InsertRange(Index: Integer; const Values: array of T); overload;
    procedure InsertRange(Index: Integer; const Source: TEnumerable<T>); overload;
    procedure Delete(Index: Integer);
    procedure DeleteRange(Index, ACount: Integer);
    function Remove(const Value: T): Integer; overload;
    function Remove(const Value: T; const AComparer: IEqualityComparer<T>): Integer; overload;
    function Extract(const Value: T): T;
    function ExtractAt(Index: Integer): T;
    procedure Exchange(Index1, Index2: Integer);
    procedure Move(CurIndex, NewIndex: Integer);
    procedure Clear;
    function Contains(const Value: T): Boolean; overload;
    function Contains(const Value: T; const AComparer: IEqualityComparer<T>): Boolean; overload;
    function IndexOf(const Value: T): Integer; overload;
    function IndexOf(const Value: T; const AComparer: IEqualityComparer<T>): Integer; overload;
    function LastIndexOf(const Value: T): Integer; overload;
    function LastIndexOf(const Value: T; const AComparer: IEqualityComparer<T>): Integer;
      overload;
    procedure Sort; overload;
    procedure Sort(const AComparer: IComparer<T>); overload;
    function BinarySearch(const Item: T; out FoundIndex: Integer): Boolean; overload;
    function BinarySearch(const Item: T; out FoundIndex: Integer;
      const AComparer: IComparer<T>): Boolean; overload;
    procedure Reverse;
    function ToArray: TArray<T>; override;
    function First: T;
    function Last: T;
    procedure TrimExcess;
    function GetEnumerator: TEnumerator;
    property Count: Integer read FCount write SetCount;
    property Capacity: Integer read GetCapacity write SetCapacity;
    property Items[Index: Integer]: T read GetItem write SetItem; default;
    property Items[const Range: TIndexRange]: TArray<T> read GetRange; default;
  end;
