  // ===== TStack<T> and TQueue<T> (chapter 6) =====
type
  // Last in, first out. The items are kept bottom first in FItems[0..FCount - 1]; the slots
  // past FCount hold Default(T), so that a taken item is let go at once.
  TStack<T> = class(TEnumerable<T>)
  private
    FItems: TArray<T>;
    FCount: Integer;
    // Changed by every operation that changes the structure (3.3).
    FVersion: Integer;
    // Whether Notify is called (NOTIFY_*): learned when there is first something to tell (12.1).
    FNotify: Integer;
    function GetCapacity: Integer;
    procedure SetCapacity(Value: Integer);
    procedure CheckNotEmpty;
    function Notifies: Boolean;
    procedure NotifyItem(const Item: T; Action: TCollectionNotification);
    // Pushes Values in order, after making room for all of them (the constructors). All of
    // them are told to Notify (cnAdding) before any goes in, so that a refused one leaves the
    // stack empty and frees none of the others.
    procedure PushAll(const Values: array of T);
    // Push without telling Notify.
    procedure PushItem(const Value: T);
    // Tells Notify that Items[From..] were removed, in order; when one call raises, the rest are
    // still told, and the exceptions overlap (9.1).
    procedure ItemsRemoved(const Items: array of T; From: Integer);
    // Clear, and when the stack is being freed (not KeepCapacity) also lets go of the array.
    procedure RemoveAll(KeepCapacity: Boolean);
  protected
    // Told cnAdding before Item enters the stack (Push; raising refuses it, and the stack is left
    // as it was), and cnRemoved after Item has left it at Clear and when the stack is freed,
    // bottom first; not by Pop and Extract, which hand the item over. Does nothing here, and is
    // only called when a descendant overrides it; an owning stack frees the item (9.1, 9.4,
    // 12.1).
    procedure Notify(const Item: T; Action: TCollectionNotification); virtual;
  public
    type
      // Walks the items bottom first (3.4); raises when the stack was changed (3.3).
      TEnumerator = record
      private
        FStack: TStack<T>;
        FIndex: Integer;
        FVersion: Integer;
        function GetCurrent: T;
      public
        function MoveNext: Boolean;
        property Current: T read GetCurrent;
      end;
    constructor Create; overload;
    constructor Create(const Values: array of T); overload;
    constructor Create(const Source: TEnumerable<T>); overload;
    destructor Destroy; override;
    procedure Push(const Value: T);
    function Pop: T;
    function Peek: T;
    // The same as Pop; an owning stack does not free the item it returns (9.1).
    function Extract: T;
    procedure Clear;
    procedure TrimExcess;
    function ToArray: TArray<T>; override;
    function GetEnumerator: TEnumerator;
    property Count: Integer read FCount;
    property Capacity: Integer read GetCapacity write SetCapacity;
  end;

  // First in, first out. The items are kept in a ring: the first is FItems[FHead] and the
  // others follow it, wrapping round to FItems[0]. The slots outside the items hold Default(T).
  TQueue<T> = class(TEnumerable<T>)
  private
    FItems: TArray<T>;
    FHead: Integer;
    FCount: Integer;
    // Changed by every operation that changes the structure (3.3).
    FVersion: Integer;
    // Whether Notify is called (NOTIFY_*): learned when there is first something to tell (12.1).
    FNotify: Integer;
    function GetCapacity: Integer;
    procedure SetCapacity(Value: Integer);
    procedure CheckNotEmpty;
    function Notifies: Boolean;
    procedure NotifyItem(const Item: T; Action: TCollectionNotification);
    // Enqueues Values in order, after making room for all of them (the constructors). All of
    // them are told to Notify (cnAdding) before any goes in, so that a refused one leaves the
    // queue empty and frees none of the others.
    procedure EnqueueAll(const Values: array of T);
    // Enqueue without telling Notify.
    procedure EnqueueItem(const Value: T);
    // The slot of the item at Index (0 is the first).
    function SlotOf(Index: Integer): Integer;
    // Tells Notify that Items[From..] were removed, in order; when one call raises, the rest are
    // still told, and the exceptions overlap (9.1).
    procedure ItemsRemoved(const Items: array of T; From: Integer);
    // Clear, and when the queue is being freed (not KeepCapacity) also lets go of the array.
    procedure RemoveAll(KeepCapacity: Boolean);
  protected
    // Told cnAdding before Item enters the queue (Enqueue; raising refuses it, and the queue is
    // left as it was), and cnRemoved after Item has left it at Clear and when the queue is freed,
    // first to last; not by Dequeue and Extract, which hand the item over. Does nothing here, and
    // is only called when a descendant overrides it; an owning queue frees the item (9.1, 9.4,
    // 12.1).
    procedure Notify(const Item: T; Action: TCollectionNotification); virtual;
  public
    type
      // Walks the items from the first (3.4); raises when the queue was changed (3.3).
      TEnumerator = record
      private
        FQueue: TQueue<T>;
        FIndex: Integer;
        FVersion: Integer;
        function GetCurrent: T;
      public
        function MoveNext: Boolean;
        property Current: T read GetCurrent;
      end;
    constructor Create; overload;
    constructor Create(const Values: array of T); overload;
    constructor Create(const Source: TEnumerable<T>); overload;
    destructor Destroy; override;
    procedure Enqueue(const Value: T);
    function Dequeue: T;
    function Peek: T;
    // The same as Dequeue; an owning queue does not free the item it returns (9.1).
    function Extract: T;
    procedure Clear;
    procedure TrimExcess;
    function ToArray: TArray<T>; override;
    function GetEnumerator: TEnumerator;
    property Count: Integer read FCount;
    property Capacity: Integer read GetCapacity write SetCapacity;
  end;
