unit System.Generics.Collections;

// The containers (docs/spec/collections.md). Written from the specification only, not from
// any other implementation.
//
// Both sections are split into one region per container, in the same order, so that the
// containers can be filled in separately. Keep each container's declarations and bodies inside
// its own region; anything shared by several containers goes in the common region.

interface

uses
  System.SysUtils, System.Generics.Defaults;

type
  // ===== Common (chapter 4) =====

  // The common parent of the containers (4.1): another container can be passed where a
  // TEnumerable<T> is taken. Iterating a value whose static type is TEnumerable<T> walks a copy
  // made by ToArray; the containers declare their own GetEnumerator, which does not copy.
  TEnumerable<T> = class abstract
  public
    type
      // Walks an array made by ToArray.
      TArrayEnumerator = record
      private
        FItems: TArray<T>;
        FIndex: Integer;
        function GetCurrent: T;
      public
        function MoveNext: Boolean;
        property Current: T read GetCurrent;
      end;
    // The items in enumeration order, as a new array.
    function ToArray: TArray<T>; virtual; abstract;
    function GetEnumerator: TArrayEnumerator;
  end;

  // What a container tells its Notify (KeyNotify, ValueNotify) about an item (9.4, 12.1):
  // cnAdding before the item goes in (raising refuses it), cnRemoved after it has left.
  TCollectionNotification = (cnAdding, cnRemoved);

  // An entry of a dictionary (4.2).
  TPair<K, V> = record
    Key: K;
    Value: V;
    constructor Create(const AKey: K; const AValue: V);
  end;

  // ===== TList<T> (chapter 5) =====

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

  // ===== TStack<T> and TQueue<T> (chapter 6) =====

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

  // ===== TDictionary<K, V> (chapter 7) =====

  // A hash table that keeps its entries in insertion order (7.3). The entries live in four
  // arrays in the order they were added (the keys, the values, the hashes and the next entries;
  // entry I is at I in each); removing one leaves a hole there, and the holes are squeezed out
  // when the entries are placed again. Each bucket holds the first entry of a chain linked
  // through FNext, which HashFind / HashLookup follow and HashRelink makes again (the layout of
  // spec/bytecode.md 2.23). An entry remembers the hash of its key, which placing the entries
  // again (and, in debug builds, every entry a lookup or an enumeration touches) checks against
  // the hash of the key as it is now, unless the hash of a key of its type cannot change
  // (IsPlainKey; 7.5).
  TDictionary<K, V> = class(TEnumerable<TPair<K, V>>)
  private
    const
      HOLE = -2;
      // Removing leaves holes; more than this many (and more holes than entries) are squeezed
      // out by the removal itself.
      MAX_IDLE_HOLES = 16;
  public
    type
      TPairEnumerator = record
      private
        FDict: TDictionary<K, V>;
        FIndex: Integer;
        FVersion: Integer;
        function GetCurrent: TPair<K, V>;
      public
        function MoveNext: Boolean;
        property Current: TPair<K, V> read GetCurrent;
      end;

      TKeyEnumerator = record
      private
        FDict: TDictionary<K, V>;
        FIndex: Integer;
        FVersion: Integer;
        function GetCurrent: K;
      public
        function MoveNext: Boolean;
        property Current: K read GetCurrent;
      end;

      TValueEnumerator = record
      private
        FDict: TDictionary<K, V>;
        FIndex: Integer;
        FVersion: Integer;
        function GetCurrent: V;
      public
        function MoveNext: Boolean;
        property Current: V read GetCurrent;
      end;

      // The keys of a dictionary, as they are now (7.3). Made and owned by the dictionary only.
      TKeyCollection = class(TEnumerable<K>)
      private
        FDict: TDictionary<K, V>;
        function GetCount: Integer;
        constructor Create(ADict: TDictionary<K, V>);
      public
        destructor Destroy; override;
        function ToArray: TArray<K>; override;
        function GetEnumerator: TKeyEnumerator;
        property Count: Integer read GetCount;
      end;

      // The values of a dictionary, as they are now (7.3). Made and owned by the dictionary only.
      TValueCollection = class(TEnumerable<V>)
      private
        FDict: TDictionary<K, V>;
        function GetCount: Integer;
        constructor Create(ADict: TDictionary<K, V>);
      public
        destructor Destroy; override;
        function ToArray: TArray<V>; override;
        function GetEnumerator: TValueEnumerator;
        property Count: Integer read GetCount;
      end;
  private
    // The first entry of each bucket (-1 when empty; a power of 2 of them, or none). Then the
    // entries in insertion order, holes included: four arrays whose length is the capacity, the
    // hash of each key as it was added, the next entry in the same bucket (-1 at the end of the
    // chain, or HOLE for an entry that was removed), the keys and the values.
    // HashLookup reads the first four fields, which must stay declared one after another in
    // this order (spec/bytecode.md 2.23).
    FBuckets: TArray<Integer>;
    FHashes: TArray<Int64>;
    FNext: TArray<Integer>;
    FKeyItems: TArray<K>;
    FValueItems: TArray<V>;
    // How many entries are in use (live entries and holes).
    FUsed: Integer;
    FCount: Integer;
    // Length(FBuckets) - 1, which takes the bucket of a hash (-1 when there are no buckets), and
    // Length(FHashes): kept beside the arrays so that adding and removing read no lengths. Set
    // again wherever the arrays are made again (Place, RemoveAll).
    FMask: Integer;
    FCapacity: Integer;
    FComparer: IEqualityComparer<K>;
    // Whether FComparer is nil (the keys are compared with KeyEquals / KeyHash): kept apart so
    // that a lookup does not compare the interface with nil.
    FByKey: Boolean;
    // Changed by every change of the structure (3.3).
    FVersion: Integer;
    // Whether KeyNotify and ValueNotify are called (NOTIFY_*): learned when there is first
    // something to tell (12.1).
    FKeyNotify: Integer;
    FValueNotify: Integer;
    // The views; set back to nil when the user frees one by mistake.
    FKeys: TKeyCollection;
    FValues: TValueCollection;
    // The container named in the messages, and its kind in the capacity messages: the
    // dictionary itself, or the THashSet<T> built on it (chapter 8).
    FOwner: TObject;
    FKind: string;
    function OwnerName: string;
    function HashOf(const Key: K): Int64;
    function BucketOf(Hash: Int64): Integer;
    function ChangedKeyError(Index: Integer): EInvalidOperation;
    procedure CheckEntry(Index: Integer);
    procedure CheckVersion(Version: Integer);
    function Notifies: Boolean;
    procedure EntryAdding(const Key: K; const Value: V);
    procedure Init(ACapacity: Integer; const AComparer: IEqualityComparer<K>);
    function NextLive(Index: Integer): Integer;
    // Sets Index to the entry whose key equals Key, or -1, and returns the hash of Key when it is
    // -1 (adding needs it, even when the dictionary is empty; when it is found, any value).
    function Find(const Key: K; out Index: Integer): Int64;
    function FindCompared(const Key: K; out Index: Integer): Int64;
    // The entry whose key equals Key, or -1: Find without the hash, for the lookups.
    function IndexOfKey(const Key: K): Integer;
    // Small enough to be inlined into its callers; making room is left to MakeRoom.
    procedure Append(const Key: K; const Value: V; Hash: Int64);
    procedure MakeRoom;
    // Removes the entry at Index; when Tell, KeyNotify and ValueNotify hear of it afterwards
    // (RemoveTelling).
    procedure RemoveAt(Index: Integer; Tell: Boolean);
    procedure RemoveTelling(Index: Integer);
    // Clear, and when the dictionary is being freed (not KeepCapacity) also lets go of the
    // arrays instead of making new ones.
    procedure RemoveAll(KeepCapacity: Boolean);
    procedure CheckAll(Skip: Integer);
    // Moves the live entries to the front in place (their order kept) and lets go of the keys and
    // values left behind; FUsed becomes FCount. The chains are left to the caller.
    procedure MoveLiveToFront;
    procedure Place(NewCapacity: Integer);
    procedure Rehash(NewCapacity: Integer);
    // Squeezes the holes out in place, keeping the capacity: the time is proportional to the
    // entries in use, not to the capacity, so that removing stays O(1) on average (11.1).
    procedure SqueezeHoles;
    function GetItem(const Key: K): V;
    procedure SetItem(const Key: K; const Value: V);
    function GetCapacity: Integer;
    procedure SetCapacity(Value: Integer);
    function GetKeys: TKeyCollection;
    function GetValues: TValueCollection;
    function KeyArray: TArray<K>;
    function TryRemove(const Key: K): Boolean;
    // Tells KeyNotify and ValueNotify that the entry left (the key, then the value even when
    // telling the key raised).
    procedure EntryRemoved(const Key: K; const Value: V);
    // Tells them that Items[From..] were removed, in order; when one call raises, the rest are
    // still told, and the exceptions overlap (9.4).
    procedure ItemsRemoved(const Items: array of TPair<K, V>; From: Integer);
    // Tells them that AddOrSetValue or Items[K] := V wrote NewValue over OldValue: Kept is the
    // key the dictionary keeps, NewKey the key passed in (equal to Kept, maybe another
    // instance). The value written over, then the key passed in, leave, unless they are NewValue
    // and Kept themselves (9.5).
    procedure EntryReplaced(const Kept, NewKey: K; const OldValue, NewValue: V);
  protected
    // Told cnAdding before a key goes in (Add, TryAdd when the key is new, AddOrSetValue,
    // Items[K] := V, the Create taking entries; AddOrSetValue and Items[K] := V with a key
    // already there too, as the key passed in is then let go), raising refuses it, and the
    // dictionary is left as it was; and cnRemoved after the key has left (Remove, Clear, freeing
    // the dictionary, the key passed in to a key already there unless it is the key kept), but
    // not when it is taken out (ExtractPair). Does nothing here, and is only called when a
    // descendant overrides it; an owning dictionary frees the key (9.3, 9.5, 12.1).
    procedure KeyNotify(const Key: K; Action: TCollectionNotification); virtual;
    // The same for the values: cnRemoved also after AddOrSetValue or Items[K] := V wrote another
    // value over it (not when it is the value written).
    procedure ValueNotify(const Value: V; Action: TCollectionNotification); virtual;
  public
    constructor Create; overload;
    constructor Create(ACapacity: Integer); overload;
    constructor Create(const AComparer: IEqualityComparer<K>); overload;
    constructor Create(ACapacity: Integer; const AComparer: IEqualityComparer<K>); overload;
    constructor Create(const Values: array of TPair<K, V>); overload;
    constructor Create(const Source: TEnumerable<TPair<K, V>>); overload;
    destructor Destroy; override;
    procedure Add(const Key: K; const Value: V);
    function TryAdd(const Key: K; const Value: V): Boolean;
    procedure AddOrSetValue(const Key: K; const Value: V);
    procedure Remove(const Key: K);
    function ExtractPair(const Key: K): TPair<K, V>;
    procedure Clear;
    function TryGetValue(const Key: K; out Value: V): Boolean;
    function ContainsKey(const Key: K): Boolean;
    function ContainsValue(const Value: V): Boolean;
    procedure TrimExcess;
    function ToArray: TArray<TPair<K, V>>; override;
    function GetEnumerator: TPairEnumerator;
    property Items[const Key: K]: V read GetItem write SetItem; default;
    property Count: Integer read FCount;
    property Capacity: Integer read GetCapacity write SetCapacity;
    property Keys: TKeyCollection read GetKeys;
    property Values: TValueCollection read GetValues;
  end;

  // ===== THashSet<T> (chapter 8) =====

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

  // ===== Owning containers (chapter 9) =====

  // A list that owns its items (9.1). OwnsObjects (True unless the constructor says otherwise)
  // makes it free the items that leave it (Delete, DeleteRange, Remove, Clear, a smaller Count,
  // the item replaced by Items[I] := X unless it is X itself, freeing the list); Extract and
  // ExtractAt hand the item over without freeing it. With T = TObject, an owning list refuses
  // instances of reference counted classes (9.2).
  TObjectList<T: class, NotRefCounted> = class(TList<T>)
  private
    FOwnsObjects: Boolean;
    procedure SetOwnsObjects(Value: Boolean);
  protected
    procedure Notify(const Item: T; Action: TCollectionNotification); override;
  public
    constructor Create; overload;
    constructor Create(AOwnsObjects: Boolean); overload;
    constructor Create(const AComparer: IComparer<T>); overload;
    constructor Create(const AComparer: IComparer<T>; AOwnsObjects: Boolean); overload;
    constructor Create(const Values: array of T); overload;
    constructor Create(const Values: array of T; AOwnsObjects: Boolean); overload;
    constructor Create(const Source: TEnumerable<T>); overload;
    constructor Create(const Source: TEnumerable<T>; AOwnsObjects: Boolean); overload;
    property OwnsObjects: Boolean read FOwnsObjects write SetOwnsObjects;
  end;

  // A stack that owns its items (9.1). OwnsObjects (True unless the constructor says otherwise)
  // makes it free the items that leave it (Pop, Clear, freeing the stack); Extract and Pop
  // called through TStack<T> hand the item over without freeing it. With T = TObject, an owning
  // stack refuses instances of reference counted classes (9.2).
  TObjectStack<T: class, NotRefCounted> = class(TStack<T>)
  private
    FOwnsObjects: Boolean;
    procedure SetOwnsObjects(Value: Boolean);
  protected
    procedure Notify(const Item: T; Action: TCollectionNotification); override;
  public
    constructor Create; overload;
    constructor Create(AOwnsObjects: Boolean); overload;
    constructor Create(const Values: array of T); overload;
    constructor Create(const Values: array of T; AOwnsObjects: Boolean); overload;
    constructor Create(const Source: TEnumerable<T>); overload;
    constructor Create(const Source: TEnumerable<T>; AOwnsObjects: Boolean); overload;
    // Takes the top item off and frees it when OwnsObjects. A procedure, so that a freed
    // instance is never handed out (9.1); Extract hands the item over.
    procedure Pop; reintroduce;
    property OwnsObjects: Boolean read FOwnsObjects write SetOwnsObjects;
  end;

  // A queue that owns its items (9.1). OwnsObjects (True unless the constructor says otherwise)
  // makes it free the items that leave it (Dequeue, Clear, freeing the queue); Extract and
  // Dequeue called through TQueue<T> hand the item over without freeing it. With T = TObject, an
  // owning queue refuses instances of reference counted classes (9.2).
  TObjectQueue<T: class, NotRefCounted> = class(TQueue<T>)
  private
    FOwnsObjects: Boolean;
    procedure SetOwnsObjects(Value: Boolean);
  protected
    procedure Notify(const Item: T; Action: TCollectionNotification); override;
  public
    constructor Create; overload;
    constructor Create(AOwnsObjects: Boolean); overload;
    constructor Create(const Values: array of T); overload;
    constructor Create(const Values: array of T; AOwnsObjects: Boolean); overload;
    constructor Create(const Source: TEnumerable<T>); overload;
    constructor Create(const Source: TEnumerable<T>; AOwnsObjects: Boolean); overload;
    // Takes the first item off and frees it when OwnsObjects. A procedure, so that a freed
    // instance is never handed out (9.1); Extract hands the item over.
    procedure Dequeue; reintroduce;
    property OwnsObjects: Boolean read FOwnsObjects write SetOwnsObjects;
  end;

  TDictionaryOwnership = (doOwnsKeys, doOwnsValues);
  TDictionaryOwnerships = set of TDictionaryOwnership;

  // A dictionary that owns its keys, its values or both (9.3). An owned key or value is freed
  // when its entry leaves the dictionary (Remove, Clear, freeing the dictionary), and an owned
  // value when AddOrSetValue or Items[K] := V writes another one over it; ExtractPair hands the
  // entry over without freeing it. Writing to a key already there keeps the key the dictionary
  // holds and frees the one passed in, unless it is the same instance. Owning the side that is
  // not a class raises at creation (E0768 when Ownerships is a constant). With K or V = TObject,
  // an owned side refuses instances of reference counted classes (9.2).
  TObjectDictionary<K: NotRefCounted; V: NotRefCounted> = class(TDictionary<K, V>)
  private
    FOwnerships: TDictionaryOwnerships;
    procedure SetOwnerships(Value: TDictionaryOwnerships);
  protected
    procedure KeyNotify(const Key: K; Action: TCollectionNotification); override;
    procedure ValueNotify(const Value: V; Action: TCollectionNotification); override;
  public
    constructor Create(Ownerships: TDictionaryOwnerships); overload;
    constructor Create(Ownerships: TDictionaryOwnerships; ACapacity: Integer); overload;
    constructor Create(Ownerships: TDictionaryOwnerships;
      const AComparer: IEqualityComparer<K>); overload;
    constructor Create(Ownerships: TDictionaryOwnerships; ACapacity: Integer;
      const AComparer: IEqualityComparer<K>); overload;
    property Ownerships: TDictionaryOwnerships read FOwnerships;
  end;

  // ===== TArray (10.4) =====

  // Sorting and binary search of arrays (10.3, 10.4). Only class methods; never instantiated.
  TArray = class
  private
    // Whether to compare with the default order of T (KeyCompare): when Comparer is nil, which
    // raises EInvalidOperation at once when T has none, even before any two values are compared.
    // The default order is compared directly, not through a comparer object.
    class function ByDefaultOrder<T>(const Comparer: IComparer<T>): Boolean; static;
    // Sorts Items stably (a bottom-up merge sort over runs sorted by insertion). Every slot is
    // written once per pass from a slot of the previous pass, so a comparer that breaks its
    // contract cannot lose or repeat an item; Items is left in an unspecified state when the
    // comparer raises, and the callers then throw it away.
    class procedure SortItems<T>(var Items: TArray<T>; const Comparer: IComparer<T>); static;
    // The binary search of Values[0 .. Count - 1] (10.3): the smallest index of an item equal to
    // Item, else the index Item would be inserted at.
    class function SearchItems<T>(const Values: array of T; Count: Integer; const Item: T;
      out FoundIndex: Integer; const Comparer: IComparer<T>): Boolean; static;
  public
    class procedure Sort<T>(var Values: array of T); overload; static;
    class procedure Sort<T>(var Values: array of T; const Comparer: IComparer<T>); overload;
      static;
    class procedure Sort<T>(var Values: array of T; const Comparer: IComparer<T>;
      Index, Count: Integer); overload; static;
    class function BinarySearch<T>(const Values: array of T; const Item: T;
      out FoundIndex: Integer): Boolean; overload; static;
    class function BinarySearch<T>(const Values: array of T; const Item: T;
      out FoundIndex: Integer; const Comparer: IComparer<T>): Boolean; overload; static;
  end;

implementation

// ===== Common (chapter 4) =====

function TEnumerable<T>.TArrayEnumerator.GetCurrent: T;
begin
  Result := FItems[FIndex];
end;

function TEnumerable<T>.TArrayEnumerator.MoveNext: Boolean;
begin
  Inc(FIndex);
  Result := FIndex < Length(FItems);
end;

function TEnumerable<T>.GetEnumerator: TArrayEnumerator;
begin
  Result := Default(TArrayEnumerator);
  Result.FItems := ToArray;
  Result.FIndex := -1;
end;

constructor TPair<K, V>.Create(const AKey: K; const AValue: V);
begin
  Key := AKey;
  Value := AValue;
end;

// The capacity after Current when one more item is needed: doubled, and at least 4 (3.5).
function GrownCapacity(Current: Integer): Integer;
begin
  if Current < 2 then
    Result := 4
  else if Current > MaxInt div 2 then
    Result := MaxInt
  else
    Result := Current * 2;
end;

// Whether RemoveAll (TList<T>, TStack<T>, TQueue<T>) lets go of the whole array rather than
// writing Default(T) to each of the Count slots in use: always when the container is being freed
// (not KeepCapacity), and otherwise unless only a few of the Capacity slots are in use, where
// making a new array of the same capacity would cost more than the writes.
function ReleasesWholeArray(KeepCapacity: Boolean; Count, Capacity: Integer): Boolean;
begin
  Result := not KeepCapacity or (Count > Capacity div 4);
end;

// The exception for a capacity below the count (3.2). Kind is `stack`, `queue`, ...
function CapacityError(const Kind: string; Value, Count: Integer): ERangeError;
begin
  Result := ERangeError.Create(Format('%s capacity %d is less than the count %d',
    [Kind, Value, Count]));
end;

// The exception for a change during enumeration (3.3).
function ModifiedError(const Container: TObject): EInvalidOperation;
begin
  Result := EInvalidOperation.Create('`' + Container.ClassName +
    '` was modified during enumeration');
end;

// The change number after Version (3.3). Wraps round to 0 after MaxInt instead of overflowing
// under {$Q+}: an enumerator only compares the number for equality. The most frequent changes
// (TList<T>.Add, adding to and removing from a dictionary) write the same rule out in place, to
// save the call; TDictionary<K, V>.Append writes it as (Version + 1) and MaxInt under {$Q-},
// which gives the same number. A change of the rule changes those places too.
function NextVersion(Version: Integer): Integer;
begin
  if Version = MaxInt then
    Result := 0
  else
    Result := Version + 1;
end;

const
  // Whether a container calls its Notify (KeyNotify, ValueNotify): not known until there is
  // first something to tell, then whether the class of the container overrides it (12.1).
  NOTIFY_UNKNOWN = 0;
  NOTIFY_CALL = 1;
  NOTIFY_SKIP = 2;

// The NOTIFY_* for a method that is overridden or not.
function NotifyState(Overridden: Boolean): Integer;
begin
  if Overridden then
    Result := NOTIFY_CALL
  else
    Result := NOTIFY_SKIP;
end;

// ===== TList<T> (chapter 5) =====

function TList<T>.TEnumerator.GetCurrent: T;
begin
  Result := FList.FItems[FIndex];
end;

function TList<T>.TEnumerator.MoveNext: Boolean;
begin
  // Compared before answering False too, so that a change in the last iteration is caught.
  if FList.FVersion <> FVersion then
    raise ModifiedError(FList);
  Inc(FIndex);
  Result := FIndex < FList.FCount;
end;

function TList<T>.Notifies: Boolean;
begin
  if FNotify = NOTIFY_UNKNOWN then
    FNotify := NotifyState(IsOverridden(Notify));
  Result := FNotify = NOTIFY_CALL;
end;

// Callers skip the call when FNotify is NOTIFY_SKIP, so that a list nobody listens to pays a
// comparison only.
procedure TList<T>.NotifyItem(const Item: T; Action: TCollectionNotification);
begin
  if Notifies then
    Notify(Item, Action);
end;

procedure TList<T>.CheckIndex(Index: Integer);
begin
  if (Index < 0) or (Index >= FCount) then
    raise ERangeError.Create('list index ' + IntToStr(Index) + ' is out of range 0..' +
      IntToStr(FCount - 1));
end;

procedure TList<T>.CheckInsertIndex(Index: Integer);
begin
  if (Index < 0) or (Index > FCount) then
    raise ERangeError.Create('list index ' + IntToStr(Index) + ' is out of range 0..' +
      IntToStr(FCount));
end;

// Makes room for ACount more items (3.5).
procedure TList<T>.Grow(ACount: Integer);
var
  Needed, NewCapacity: Integer;
begin
  Needed := FCount + ACount;
  if Needed <= Length(FItems) then
    Exit;
  NewCapacity := GrownCapacity(Length(FItems));
  if NewCapacity < Needed then
    NewCapacity := Needed;
  SetLength(FItems, NewCapacity);
end;

// Appends Values after the items, then turns them into place in front of Index.
procedure TList<T>.InsertArray(Index: Integer; const Values: array of T);
var
  I, N: Integer;
begin
  CheckInsertIndex(Index);
  N := Length(Values);
  if N = 0 then
    Exit;
  // All of them are told before any goes in, so that a refused one leaves the list as it was
  if FNotify <> NOTIFY_SKIP then
    for I := 0 to N - 1 do
      NotifyItem(Values[I], TCollectionNotification.cnAdding);
  Grow(N);
  for I := 0 to N - 1 do
    FItems[FCount + I] := Values[I];
  ArrayMoveRange(FItems, FCount, Index, N);
  FCount := FCount + N;
  FVersion := NextVersion(FVersion);
end;

function TList<T>.GetItem(Index: Integer): T;
begin
  if (Index < 0) or (Index >= FCount) then
    CheckIndex(Index);
  Result := FItems[Index];
end;

procedure TList<T>.SetItem(Index: Integer; const Value: T);
var
  Old: T;
begin
  if (Index < 0) or (Index >= FCount) then
    CheckIndex(Index);
  if FNotify = NOTIFY_SKIP then
  begin
    FItems[Index] := Value;
    Exit;
  end;
  NotifyItem(Value, TCollectionNotification.cnAdding);
  Old := FItems[Index];
  FItems[Index] := Value;
  // The item written over leaves, unless it is the one written (9.1)
  if (FNotify = NOTIFY_CALL) and not SameInstance(Old, Value) then
    Notify(Old, TCollectionNotification.cnRemoved);
end;

function TList<T>.GetRange(const Range: TIndexRange): TArray<T>;
var
  AFirst, ALast: Integer;
begin
  Range.Clamp(0, FCount - 1, AFirst, ALast);
  Result := Copy(FItems, AFirst, ALast - AFirst + 1);
end;

procedure TList<T>.SetCount(Value: Integer);
var
  I: Integer;
  Removed: TArray<T>;
begin
  if Value < 0 then
    raise ERangeError.Create('list count ' + IntToStr(Value) + ' is out of range');
  if Value > FCount then
  begin
    Grow(Value - FCount);
    FCount := Value;
    FVersion := NextVersion(FVersion);
    Exit;
  end;
  if Notifies then
    Removed := Copy(FItems, Value, FCount - Value)
  else
    Removed := nil;
  for I := Value to FCount - 1 do
    FItems[I] := Default(T);
  FCount := Value;
  FVersion := NextVersion(FVersion);
  ItemsRemoved(Removed, 0);
end;

function TList<T>.GetCapacity: Integer;
begin
  Result := Length(FItems);
end;

procedure TList<T>.SetCapacity(Value: Integer);
begin
  if Value < FCount then
    raise CapacityError('list', Value, FCount);
  if Value <> Length(FItems) then
  begin
    SetLength(FItems, Value);
    FVersion := NextVersion(FVersion);
  end;
end;

constructor TList<T>.Create;
begin
  inherited Create;
end;

constructor TList<T>.Create(const AComparer: IComparer<T>);
begin
  inherited Create;
  FComparer := AComparer;
end;

// Both take exactly the capacity of the items (3.5).
constructor TList<T>.Create(const Values: array of T);
begin
  inherited Create;
  SetLength(FItems, Length(Values));
  AddRange(Values);
end;

constructor TList<T>.Create(const Source: TEnumerable<T>);
var
  Values: TArray<T>;
begin
  inherited Create;
  Values := Source.ToArray;
  SetLength(FItems, Length(Values));
  AddRange(Values);
end;

destructor TList<T>.Destroy;
begin
  RemoveAll(False);
  inherited Destroy;
end;

procedure TList<T>.Notify(const Item: T; Action: TCollectionNotification);
begin
end;

procedure TList<T>.ItemsRemoved(const Items: array of T; From: Integer);
var
  I: Integer;
  Done: Boolean;
begin
  for I := From to High(Items) do
  begin
    Done := False;
    try
      Notify(Items[I], TCollectionNotification.cnRemoved);
      Done := True;
    finally
      // An exception raised by the rest overlaps this one (Suppressed)
      if not Done then
        ItemsRemoved(Items, I + 1);
    end;
  end;
end;

function TList<T>.Add(const Value: T): Integer;
begin
  if FNotify <> NOTIFY_SKIP then
    NotifyItem(Value, TCollectionNotification.cnAdding);
  if FCount = Length(FItems) then
    Grow(1);
  FItems[FCount] := Value;
  Result := FCount;
  FCount := FCount + 1;
  // NextVersion, written out
  if FVersion = MaxInt then
    FVersion := 0
  else
    FVersion := FVersion + 1;
end;

procedure TList<T>.AddRange(const Values: array of T);
begin
  InsertArray(FCount, Values);
end;

procedure TList<T>.AddRange(const Source: TEnumerable<T>);
begin
  InsertArray(FCount, Source.ToArray);
end;

procedure TList<T>.Insert(Index: Integer; const Value: T);
begin
  CheckInsertIndex(Index);
  if FNotify <> NOTIFY_SKIP then
    NotifyItem(Value, TCollectionNotification.cnAdding);
  if FCount = Length(FItems) then
    Grow(1);
  FItems[FCount] := Value;
  ArrayMoveRange(FItems, FCount, Index, 1);
  FCount := FCount + 1;
  FVersion := NextVersion(FVersion);
end;

procedure TList<T>.InsertRange(Index: Integer; const Values: array of T);
begin
  // Values cannot be FItems itself (it is private, and ToArray and slices copy it).
  InsertArray(Index, Values);
end;

procedure TList<T>.InsertRange(Index: Integer; const Source: TEnumerable<T>);
begin
  InsertArray(Index, Source.ToArray);
end;

procedure TList<T>.Delete(Index: Integer);
var
  Item: T;
begin
  CheckIndex(Index);
  Item := FItems[Index];
  RemoveAt(Index);
  if FNotify <> NOTIFY_SKIP then
    NotifyItem(Item, TCollectionNotification.cnRemoved);
end;

procedure TList<T>.RemoveAt(Index: Integer);
begin
  ArrayMoveRange(FItems, Index, FCount - 1, 1);
  FItems[FCount - 1] := Default(T);
  FCount := FCount - 1;
  FVersion := NextVersion(FVersion);
end;

procedure TList<T>.DeleteRange(Index, ACount: Integer);
var
  I: Integer;
  Removed: TArray<T>;
begin
  if ACount = 0 then
    Exit;
  if ACount < 0 then
    raise ERangeError.Create('list count ' + IntToStr(ACount) + ' is out of range');
  if ACount = 1 then
    CheckIndex(Index)
  else if (Index < 0) or (Index > FCount - ACount) then
    raise ERangeError.Create('list range ' + IntToStr(Index) + '..' +
      IntToStr(Int64(Index) + ACount - 1) + ' is out of range 0..' + IntToStr(FCount - 1));
  if Notifies then
    Removed := Copy(FItems, Index, ACount)
  else
    Removed := nil;
  // Turn the removed items to the end, then let the slots go.
  ArrayMoveRange(FItems, Index, FCount - ACount, ACount);
  for I := FCount - ACount to FCount - 1 do
    FItems[I] := Default(T);
  FCount := FCount - ACount;
  FVersion := NextVersion(FVersion);
  ItemsRemoved(Removed, 0);
end;

function TList<T>.Remove(const Value: T): Integer;
begin
  Result := IndexOf(Value);
  if Result >= 0 then
    Delete(Result);
end;

function TList<T>.Remove(const Value: T; const AComparer: IEqualityComparer<T>): Integer;
begin
  Result := IndexOf(Value, AComparer);
  if Result >= 0 then
    Delete(Result);
end;

function TList<T>.Extract(const Value: T): T;
var
  Index: Integer;
begin
  Index := IndexOf(Value);
  if Index < 0 then
    Result := Default(T)
  else
    Result := ExtractAt(Index);
end;

function TList<T>.ExtractAt(Index: Integer): T;
begin
  CheckIndex(Index);
  Result := FItems[Index];
  RemoveAt(Index);
end;

procedure TList<T>.Exchange(Index1, Index2: Integer);
var
  Temp: T;
begin
  CheckIndex(Index1);
  CheckIndex(Index2);
  Temp := FItems[Index1];
  FItems[Index1] := FItems[Index2];
  FItems[Index2] := Temp;
  FVersion := NextVersion(FVersion);
end;

procedure TList<T>.Move(CurIndex, NewIndex: Integer);
begin
  CheckIndex(CurIndex);
  CheckIndex(NewIndex);
  ArrayMoveRange(FItems, CurIndex, NewIndex, 1);
  FVersion := NextVersion(FVersion);
end;

procedure TList<T>.Clear;
begin
  RemoveAll(True);
end;

procedure TList<T>.RemoveAll(KeepCapacity: Boolean);
var
  I, N, Count: Integer;
  Tell, WholeArray: Boolean;
  Removed: TArray<T>;
begin
  Tell := Notifies;
  Count := FCount;
  // Letting go of the whole array (and making a new one of the same capacity, all Default(T))
  // writes no item, which is cheaper than writing Default(T) to each one unless only a few of
  // the slots are in use. The items are let go of in index order either way.
  N := Length(FItems);
  WholeArray := ReleasesWholeArray(KeepCapacity, Count, N);
  // When the slots are written one at a time, the items are copied out first and let go of
  // from the copy, so that writing a slot lets go of nothing and an item a destructor adds to a
  // slot is not written over afterwards
  if Tell or not WholeArray then
    Removed := Copy(FItems, 0, Count)
  else
    Removed := nil;
  // The list holds no items while they are let go of, so that a destructor that looks at the
  // list sees it empty whichever way they go, and the items a destructor adds stay in the list
  // (collections.md 9.4).
  FCount := 0;
  FVersion := NextVersion(FVersion);
  if WholeArray then
  begin
    // The field is nil before the array is let go of (an assignment lets go of the old value
    // after writing), so the items a destructor adds go into an array of their own, which is
    // kept (it may already be larger than N)
    FItems := nil;
    if KeepCapacity and (Length(FItems) < N) then
      SetLength(FItems, N);
  end
  else
    for I := 0 to Count - 1 do
      FItems[I] := Default(T);
  if Tell then
    ItemsRemoved(Removed, 0);
end;

function TList<T>.Contains(const Value: T): Boolean;
begin
  Result := IndexOf(Value) >= 0;
end;

function TList<T>.Contains(const Value: T; const AComparer: IEqualityComparer<T>): Boolean;
begin
  Result := IndexOf(Value, AComparer) >= 0;
end;

function TList<T>.IndexOf(const Value: T): Integer;
var
  I: Integer;
begin
  for I := 0 to FCount - 1 do
    if KeyEquals<T>(FItems[I], Value) then
      Exit(I);
  Result := -1;
end;

function TList<T>.IndexOf(const Value: T; const AComparer: IEqualityComparer<T>): Integer;
var
  I: Integer;
begin
  if AComparer = nil then
    Exit(IndexOf(Value));
  for I := 0 to FCount - 1 do
    if AComparer.Equals(FItems[I], Value) then
      Exit(I);
  Result := -1;
end;

function TList<T>.LastIndexOf(const Value: T): Integer;
var
  I: Integer;
begin
  for I := FCount - 1 downto 0 do
    if KeyEquals<T>(FItems[I], Value) then
      Exit(I);
  Result := -1;
end;

function TList<T>.LastIndexOf(const Value: T; const AComparer: IEqualityComparer<T>): Integer;
var
  I: Integer;
begin
  if AComparer = nil then
    Exit(LastIndexOf(Value));
  for I := FCount - 1 downto 0 do
    if AComparer.Equals(FItems[I], Value) then
      Exit(I);
  Result := -1;
end;

procedure TList<T>.Sort;
begin
  Sort(FComparer);
end;

// Sorts a copy, so that a comparer that raises leaves the list as it was. A comparer that changed
// the list keeps its changes (the sorted copy is thrown away), and gets EInvalidOperation (10.3).
// Without a comparer, the elements of a type ordered by a built-in comparison are sorted in place
// by the VM (ArraySortRange), which runs no Pascal code and raises nothing once it moves them.
procedure TList<T>.Sort(const AComparer: IComparer<T>);
var
  Work: TArray<T>;
  Version, I: Integer;
begin
  if IsPlainOrder(Default(T)) then
    if AComparer = nil then
    begin
      ArraySortRange(FItems, 0, FCount);
      FVersion := NextVersion(FVersion);
      Exit;
    end;
  Work := Copy(FItems, 0, FCount);
  Version := FVersion;
  TArray.SortItems<T>(Work, AComparer);
  if FVersion <> Version then
    raise ModifiedError(Self);
  for I := 0 to FCount - 1 do
    FItems[I] := Work[I];
  FVersion := NextVersion(FVersion);
end;

function TList<T>.BinarySearch(const Item: T; out FoundIndex: Integer): Boolean;
begin
  Result := BinarySearch(Item, FoundIndex, FComparer);
end;

function TList<T>.BinarySearch(const Item: T; out FoundIndex: Integer;
  const AComparer: IComparer<T>): Boolean;
var
  Items: TArray<T>;
begin
  // The array as it is now, so that a comparer that changes the list cannot make the search read
  // past the items it was called with; an item it writes in place can still be seen (5.6).
  Items := FItems;
  Result := TArray.SearchItems<T>(Items, FCount, Item, FoundIndex, AComparer);
end;

procedure TList<T>.Reverse;
var
  I, J: Integer;
  Temp: T;
begin
  I := 0;
  J := FCount - 1;
  while I < J do
  begin
    Temp := FItems[I];
    FItems[I] := FItems[J];
    FItems[J] := Temp;
    Inc(I);
    Dec(J);
  end;
  FVersion := NextVersion(FVersion);
end;

function TList<T>.ToArray: TArray<T>;
begin
  Result := Copy(FItems, 0, FCount);
end;

function TList<T>.First: T;
begin
  if FCount = 0 then
    raise ERangeError.Create('the list is empty');
  Result := FItems[0];
end;

function TList<T>.Last: T;
begin
  if FCount = 0 then
    raise ERangeError.Create('the list is empty');
  Result := FItems[FCount - 1];
end;

procedure TList<T>.TrimExcess;
begin
  SetCapacity(FCount);
end;

function TList<T>.GetEnumerator: TEnumerator;
begin
  Result := Default(TEnumerator);
  Result.FList := Self;
  Result.FIndex := -1;
  Result.FVersion := FVersion;
end;

// ===== TStack<T> and TQueue<T> (chapter 6) =====

constructor TStack<T>.Create;
begin
  inherited Create;
end;

constructor TStack<T>.Create(const Values: array of T);
begin
  inherited Create;
  PushAll(Values);
end;

constructor TStack<T>.Create(const Source: TEnumerable<T>);
begin
  inherited Create;
  PushAll(Source.ToArray);
end;

procedure TStack<T>.PushAll(const Values: array of T);
var
  I: Integer;
begin
  if FNotify <> NOTIFY_SKIP then
    for I := 0 to High(Values) do
      NotifyItem(Values[I], TCollectionNotification.cnAdding);
  SetCapacity(Length(Values));
  for I := 0 to High(Values) do
    PushItem(Values[I]);
end;

destructor TStack<T>.Destroy;
begin
  RemoveAll(False);
  inherited Destroy;
end;

procedure TStack<T>.Notify(const Item: T; Action: TCollectionNotification);
begin
end;

function TStack<T>.Notifies: Boolean;
begin
  if FNotify = NOTIFY_UNKNOWN then
    FNotify := NotifyState(IsOverridden(Notify));
  Result := FNotify = NOTIFY_CALL;
end;

procedure TStack<T>.NotifyItem(const Item: T; Action: TCollectionNotification);
begin
  if Notifies then
    Notify(Item, Action);
end;

procedure TStack<T>.ItemsRemoved(const Items: array of T; From: Integer);
var
  I: Integer;
  Done: Boolean;
begin
  for I := From to High(Items) do
  begin
    Done := False;
    try
      Notify(Items[I], TCollectionNotification.cnRemoved);
      Done := True;
    finally
      // An exception raised by the rest overlaps this one (Suppressed)
      if not Done then
        ItemsRemoved(Items, I + 1);
    end;
  end;
end;

function TStack<T>.GetCapacity: Integer;
begin
  Result := Length(FItems);
end;

procedure TStack<T>.SetCapacity(Value: Integer);
begin
  if Value < FCount then
    raise CapacityError('stack', Value, FCount);
  if Value <> Length(FItems) then
  begin
    SetLength(FItems, Value);
    FVersion := NextVersion(FVersion);
  end;
end;

procedure TStack<T>.CheckNotEmpty;
begin
  if FCount = 0 then
    raise EListError.Create('stack is empty');
end;

procedure TStack<T>.Push(const Value: T);
begin
  if FNotify <> NOTIFY_SKIP then
    NotifyItem(Value, TCollectionNotification.cnAdding);
  PushItem(Value);
end;

procedure TStack<T>.PushItem(const Value: T);
begin
  if FCount = Length(FItems) then
    SetLength(FItems, GrownCapacity(Length(FItems)));
  FItems[FCount] := Value;
  Inc(FCount);
  FVersion := NextVersion(FVersion);
end;

function TStack<T>.Pop: T;
begin
  CheckNotEmpty;
  Dec(FCount);
  Result := FItems[FCount];
  FItems[FCount] := Default(T);
  FVersion := NextVersion(FVersion);
end;

function TStack<T>.Peek: T;
begin
  CheckNotEmpty;
  Result := FItems[FCount - 1];
end;

function TStack<T>.Extract: T;
begin
  Result := Pop;
end;

procedure TStack<T>.Clear;
begin
  RemoveAll(True);
end;

procedure TStack<T>.RemoveAll(KeepCapacity: Boolean);
var
  I, N, Count: Integer;
  Tell, WholeArray: Boolean;
  Removed: TArray<T>;
begin
  // The same as TList<T>.RemoveAll: the stack holds no items while they are let go of, bottom
  // first, and the items a destructor pushes stay in the stack (collections.md 9.4)
  Tell := Notifies;
  Count := FCount;
  N := Length(FItems);
  WholeArray := ReleasesWholeArray(KeepCapacity, Count, N);
  if Tell or not WholeArray then
    Removed := Copy(FItems, 0, Count)
  else
    Removed := nil;
  FCount := 0;
  FVersion := NextVersion(FVersion);
  if WholeArray then
  begin
    // The field is nil before the array is let go of, so the items a destructor pushes go into
    // an array of their own, which is kept (it may already be larger than N)
    FItems := nil;
    if KeepCapacity and (Length(FItems) < N) then
      SetCapacity(N);
  end
  else
    for I := 0 to Count - 1 do
      FItems[I] := Default(T);
  if Tell then
    ItemsRemoved(Removed, 0);
end;

procedure TStack<T>.TrimExcess;
begin
  SetCapacity(FCount);
end;

function TStack<T>.ToArray: TArray<T>;
begin
  Result := Copy(FItems, 0, FCount);
end;

function TStack<T>.GetEnumerator: TEnumerator;
begin
  Result := Default(TEnumerator);
  Result.FStack := Self;
  Result.FIndex := -1;
  Result.FVersion := FVersion;
end;

function TStack<T>.TEnumerator.GetCurrent: T;
begin
  Result := FStack.FItems[FIndex];
end;

function TStack<T>.TEnumerator.MoveNext: Boolean;
begin
  if FStack.FVersion <> FVersion then
    raise ModifiedError(FStack);
  Inc(FIndex);
  Result := FIndex < FStack.FCount;
end;

constructor TQueue<T>.Create;
begin
  inherited Create;
end;

constructor TQueue<T>.Create(const Values: array of T);
begin
  inherited Create;
  EnqueueAll(Values);
end;

constructor TQueue<T>.Create(const Source: TEnumerable<T>);
begin
  inherited Create;
  EnqueueAll(Source.ToArray);
end;

procedure TQueue<T>.EnqueueAll(const Values: array of T);
var
  I: Integer;
begin
  if FNotify <> NOTIFY_SKIP then
    for I := 0 to High(Values) do
      NotifyItem(Values[I], TCollectionNotification.cnAdding);
  SetCapacity(Length(Values));
  for I := 0 to High(Values) do
    EnqueueItem(Values[I]);
end;

destructor TQueue<T>.Destroy;
begin
  RemoveAll(False);
  inherited Destroy;
end;

procedure TQueue<T>.Notify(const Item: T; Action: TCollectionNotification);
begin
end;

function TQueue<T>.Notifies: Boolean;
begin
  if FNotify = NOTIFY_UNKNOWN then
    FNotify := NotifyState(IsOverridden(Notify));
  Result := FNotify = NOTIFY_CALL;
end;

procedure TQueue<T>.NotifyItem(const Item: T; Action: TCollectionNotification);
begin
  if Notifies then
    Notify(Item, Action);
end;

procedure TQueue<T>.ItemsRemoved(const Items: array of T; From: Integer);
var
  I: Integer;
  Done: Boolean;
begin
  for I := From to High(Items) do
  begin
    Done := False;
    try
      Notify(Items[I], TCollectionNotification.cnRemoved);
      Done := True;
    finally
      // An exception raised by the rest overlaps this one (Suppressed)
      if not Done then
        ItemsRemoved(Items, I + 1);
    end;
  end;
end;

function TQueue<T>.SlotOf(Index: Integer): Integer;
begin
  Result := FHead + Index;
  if Result >= Length(FItems) then
    Dec(Result, Length(FItems));
end;

function TQueue<T>.GetCapacity: Integer;
begin
  Result := Length(FItems);
end;

procedure TQueue<T>.SetCapacity(Value: Integer);
begin
  if Value < FCount then
    raise CapacityError('queue', Value, FCount);
  if Value <> Length(FItems) then
  begin
    // Turn the ring so that the first item is in slot 0: the items are then in
    // FItems[0..FCount - 1], and the free slots after them can be cut off or added to.
    if FHead > 0 then
      ArrayMoveRange(FItems, FHead, 0, Length(FItems) - FHead);
    FHead := 0;
    SetLength(FItems, Value);
    FVersion := NextVersion(FVersion);
  end;
end;

procedure TQueue<T>.CheckNotEmpty;
begin
  if FCount = 0 then
    raise EListError.Create('queue is empty');
end;

procedure TQueue<T>.Enqueue(const Value: T);
begin
  if FNotify <> NOTIFY_SKIP then
    NotifyItem(Value, TCollectionNotification.cnAdding);
  EnqueueItem(Value);
end;

procedure TQueue<T>.EnqueueItem(const Value: T);
begin
  if FCount = Length(FItems) then
    SetCapacity(GrownCapacity(Length(FItems)));
  FItems[SlotOf(FCount)] := Value;
  Inc(FCount);
  FVersion := NextVersion(FVersion);
end;

function TQueue<T>.Dequeue: T;
begin
  CheckNotEmpty;
  Result := FItems[FHead];
  FItems[FHead] := Default(T);
  FHead := SlotOf(1);
  Dec(FCount);
  FVersion := NextVersion(FVersion);
end;

function TQueue<T>.Peek: T;
begin
  CheckNotEmpty;
  Result := FItems[FHead];
end;

function TQueue<T>.Extract: T;
begin
  Result := Dequeue;
end;

procedure TQueue<T>.Clear;
begin
  RemoveAll(True);
end;

procedure TQueue<T>.RemoveAll(KeepCapacity: Boolean);
var
  I, N, Head, Count: Integer;
  Tell, WholeArray: Boolean;
  Removed: TArray<T>;
begin
  // The same as TList<T>.RemoveAll: the queue holds no items while they are let go of, first to
  // last, and the items a destructor enqueues stay in the queue (collections.md 9.4)
  Tell := Notifies;
  Head := FHead;
  Count := FCount;
  N := Length(FItems);
  WholeArray := ReleasesWholeArray(KeepCapacity, Count, N);
  // Letting go of the whole array goes slot by slot, which is first to last only when the ring
  // does not wrap round; otherwise the items are copied out in order and let go of from the copy
  if Tell or not WholeArray or (Head + Count > N) then
    Removed := ToArray
  else
    Removed := nil;
  FHead := 0;
  FCount := 0;
  FVersion := NextVersion(FVersion);
  if WholeArray then
  begin
    // The field is nil before the array is let go of, so the items a destructor enqueues go into
    // an array of their own, which is kept (it may already be larger than N)
    FItems := nil;
    if KeepCapacity and (Length(FItems) < N) then
      SetCapacity(N);
  end
  else
    for I := 0 to Count - 1 do
    begin
      FItems[Head] := Default(T);
      Inc(Head);
      if Head = N then
        Head := 0;
    end;
  if Tell then
    ItemsRemoved(Removed, 0);
end;

procedure TQueue<T>.TrimExcess;
begin
  SetCapacity(FCount);
end;

function TQueue<T>.ToArray: TArray<T>;
var
  I: Integer;
begin
  SetLength(Result, FCount);
  for I := 0 to FCount - 1 do
    Result[I] := FItems[SlotOf(I)];
end;

function TQueue<T>.GetEnumerator: TEnumerator;
begin
  Result := Default(TEnumerator);
  Result.FQueue := Self;
  Result.FIndex := -1;
  Result.FVersion := FVersion;
end;

function TQueue<T>.TEnumerator.GetCurrent: T;
begin
  Result := FQueue.FItems[FQueue.SlotOf(FIndex)];
end;

function TQueue<T>.TEnumerator.MoveNext: Boolean;
begin
  if FQueue.FVersion <> FVersion then
    raise ModifiedError(FQueue);
  Inc(FIndex);
  Result := FIndex < FQueue.FCount;
end;

// ===== TDictionary<K, V> (chapter 7) =====

procedure TDictionary<K, V>.Init(ACapacity: Integer; const AComparer: IEqualityComparer<K>);
begin
  FOwner := Self;
  FKind := 'dictionary';
  if ACapacity < 0 then
    raise ERangeError.Create(FKind + ' capacity ' + IntToStr(ACapacity) + ' is out of range');
  FComparer := AComparer;
  FByKey := AComparer = nil;
  Rehash(ACapacity);
end;

constructor TDictionary<K, V>.Create;
begin
  inherited Create;
  Init(0, nil);
end;

constructor TDictionary<K, V>.Create(ACapacity: Integer);
begin
  inherited Create;
  Init(ACapacity, nil);
end;

constructor TDictionary<K, V>.Create(const AComparer: IEqualityComparer<K>);
begin
  inherited Create;
  Init(0, AComparer);
end;

constructor TDictionary<K, V>.Create(ACapacity: Integer; const AComparer: IEqualityComparer<K>);
begin
  inherited Create;
  Init(ACapacity, AComparer);
end;

constructor TDictionary<K, V>.Create(const Values: array of TPair<K, V>);
var
  I: Integer;
begin
  inherited Create;
  Init(0, nil);
  for I := 0 to High(Values) do
    Add(Values[I].Key, Values[I].Value);
end;

constructor TDictionary<K, V>.Create(const Source: TEnumerable<TPair<K, V>>);
var
  Items: TArray<TPair<K, V>>;
  I: Integer;
begin
  inherited Create;
  Init(0, nil);
  Items := Source.ToArray;
  for I := 0 to High(Items) do
    Add(Items[I].Key, Items[I].Value);
end;

destructor TDictionary<K, V>.Destroy;
begin
  try
    RemoveAll(False);
  finally
    // A view the user freed by mistake has already set its field back to nil (7.3)
    FKeys.Free;
    FValues.Free;
  end;
  inherited Destroy;
end;

procedure TDictionary<K, V>.KeyNotify(const Key: K; Action: TCollectionNotification);
begin
end;

procedure TDictionary<K, V>.ValueNotify(const Value: V; Action: TCollectionNotification);
begin
end;

// Whether KeyNotify or ValueNotify is called. Callers skip the call when both are NOTIFY_SKIP,
// so that a dictionary nobody listens to pays two comparisons only.
function TDictionary<K, V>.Notifies: Boolean;
begin
  if FKeyNotify = NOTIFY_UNKNOWN then
  begin
    FKeyNotify := NotifyState(IsOverridden(KeyNotify));
    FValueNotify := NotifyState(IsOverridden(ValueNotify));
  end;
  Result := (FKeyNotify = NOTIFY_CALL) or (FValueNotify = NOTIFY_CALL);
end;

// The key, then the value, are told before they go in.
procedure TDictionary<K, V>.EntryAdding(const Key: K; const Value: V);
begin
  if not Notifies then
    Exit;
  if FKeyNotify = NOTIFY_CALL then
    KeyNotify(Key, TCollectionNotification.cnAdding);
  if FValueNotify = NOTIFY_CALL then
    ValueNotify(Value, TCollectionNotification.cnAdding);
end;

procedure TDictionary<K, V>.EntryRemoved(const Key: K; const Value: V);
begin
  try
    if FKeyNotify = NOTIFY_CALL then
      KeyNotify(Key, TCollectionNotification.cnRemoved);
  finally
    if FValueNotify = NOTIFY_CALL then
      ValueNotify(Value, TCollectionNotification.cnRemoved);
  end;
end;

procedure TDictionary<K, V>.EntryReplaced(const Kept, NewKey: K; const OldValue, NewValue: V);
begin
  try
    if (FValueNotify = NOTIFY_CALL) and not SameInstance(OldValue, NewValue) then
      ValueNotify(OldValue, TCollectionNotification.cnRemoved);
  finally
    if (FKeyNotify = NOTIFY_CALL) and not SameInstance(Kept, NewKey) then
      KeyNotify(NewKey, TCollectionNotification.cnRemoved);
  end;
end;

procedure TDictionary<K, V>.ItemsRemoved(const Items: array of TPair<K, V>; From: Integer);
var
  I: Integer;
  Done: Boolean;
begin
  for I := From to High(Items) do
  begin
    Done := False;
    try
      EntryRemoved(Items[I].Key, Items[I].Value);
      Done := True;
    finally
      // An exception raised by the rest overlaps this one (Suppressed)
      if not Done then
        ItemsRemoved(Items, I + 1);
    end;
  end;
end;

function TDictionary<K, V>.OwnerName: string;
begin
  Result := FOwner.ClassName;
end;

// Find writes this out in place.
function TDictionary<K, V>.HashOf(const Key: K): Int64;
begin
  if FByKey then
    Result := KeyHash<K>(Key)
  else
    Result := FComparer.GetHashCode(Key);
end;

// Append, RemoveAt and SqueezeHoles write this out in place (HashFind, HashLookup and HashRelink
// take the bucket of a hash the same way).
function TDictionary<K, V>.BucketOf(Hash: Int64): Integer;
begin
  Result := Integer(Hash and FMask);
end;

// The exception for the entry at Index, whose key no longer has the hash it was added with.
function TDictionary<K, V>.ChangedKeyError(Index: Integer): EInvalidOperation;
var
  I, Position: Integer;
begin
  Position := 0;
  for I := 0 to Index - 1 do
    if FNext[I] <> HOLE then
      Inc(Position);
  Result := EInvalidOperation.Create('the key of entry #' + IntToStr(Position) + ' in `' +
    OwnerName + '` was changed after it was added');
end;

// The keys whose hash cannot change in the dictionary are never checked (IsPlainKey; 7.5).
procedure TDictionary<K, V>.CheckEntry(Index: Integer);
begin
  if FByKey then
    if IsPlainKey(Default(K)) then
      Exit;
  if HashOf(FKeyItems[Index]) <> FHashes[Index] then
    raise ChangedKeyError(Index);
end;

procedure TDictionary<K, V>.CheckVersion(Version: Integer);
begin
  if Version <> FVersion then
    raise ModifiedError(FOwner);
end;

function TDictionary<K, V>.NextLive(Index: Integer): Integer;
begin
  Result := Index;
  while (Result < FUsed) and (FNext[Result] = HOLE) do
    Inc(Result);
end;

// Runs at every lookup. HashLookup hashes and compares the keys of a type whose key equality it
// knows (spec/bytecode.md 2.23), so that the lookup of such a key is this short body (small enough
// to be inlined; the hash again only for a key to add); the others go to FindCompared.
function TDictionary<K, V>.Find(const Key: K; out Index: Integer): Int64;
begin
  if IsPlainKey(Key) then
    if FByKey then
    begin
      Index := HashLookup(Key, FBuckets, FHashes, FNext, FKeyItems);
      if Index < 0 then
        Exit(KeyHash<K>(Key));
      Exit(0);
    end;
  Result := FindCompared(Key, Index);
end;

function TDictionary<K, V>.IndexOfKey(const Key: K): Integer;
begin
  if IsPlainKey(Key) then
    if FByKey then
      Exit(HashLookup(Key, FBuckets, FHashes, FNext, FKeyItems));
  FindCompared(Key, Result);
end;

// Find of the keys compared here: HashFind gives each entry with the same hash, whose key is
// compared with KeyEquals or FComparer.Equals. HashOf and CheckEntry are written out.
function TDictionary<K, V>.FindCompared(const Key: K; out Index: Integer): Int64;
var
  I: Integer;
begin
  if FByKey then
  begin
    Result := KeyHash<K>(Key);
    I := HashFind(Result, FBuckets, FHashes, FNext, -1);
    while I >= 0 do
    begin
      {$IFDEF DEBUG}
      if KeyHash<K>(FKeyItems[I]) <> Result then
        raise ChangedKeyError(I);
      {$ENDIF}
      if KeyEquals<K>(FKeyItems[I], Key) then
        Break;
      I := HashFind(Result, FBuckets, FHashes, FNext, I);
    end;
  end
  else
  begin
    Result := FComparer.GetHashCode(Key);
    I := HashFind(Result, FBuckets, FHashes, FNext, -1);
    while I >= 0 do
    begin
      {$IFDEF DEBUG}
      if FComparer.GetHashCode(FKeyItems[I]) <> Result then
        raise ChangedKeyError(I);
      {$ENDIF}
      if FComparer.Equals(FKeyItems[I], Key) then
        Break;
      I := HashFind(Result, FBuckets, FHashes, FNext, I);
    end;
  end;
  Index := I;
end;

procedure TDictionary<K, V>.Append(const Key: K; const Value: V; Hash: Int64);
var
  U: Integer;
  B: Int64;
begin
  U := FUsed;
  if U = FCapacity then
  begin
    MakeRoom;
    U := FUsed;
  end;
  B := Hash and FMask;
  FKeyItems[U] := Key;
  FValueItems[U] := Value;
  FHashes[U] := Hash;
  FNext[U] := FBuckets[B];
  FBuckets[B] := U;
  // None of these can overflow (FCount <= FUsed < FCapacity, and the version wraps from MaxInt to
  // 0 by the mask), so they are left unchecked: the checks would make Append too large to be
  // inlined (design/inlining.md 3.5). The state of {$Q} is restored afterwards.
  {$IFOPT Q+}{$DEFINE APPEND_Q_ON}{$Q-}{$ENDIF}
  FUsed := U + 1;
  Inc(FCount);
  // NextVersion, written out
  FVersion := (FVersion + 1) and MaxInt;
  {$IFDEF APPEND_Q_ON}{$Q+}{$UNDEF APPEND_Q_ON}{$ENDIF}
end;

// Makes room for one more entry when every entry is in use: the holes are squeezed out when they
// are more than half, and otherwise the capacity grows.
procedure TDictionary<K, V>.MakeRoom;
begin
  if FUsed - FCount > FUsed div 2 then
    Rehash(FCapacity)
  else if FCapacity < 2 then
    Rehash(4)
  else
    Rehash(2 * FCapacity);
end;

procedure TDictionary<K, V>.RemoveAt(Index: Integer; Tell: Boolean);
var
  B: Int64;
  Holes, I, Next: Integer;
  Squeeze: Boolean;
begin
  if Tell then
    if (FKeyNotify <> NOTIFY_SKIP) or (FValueNotify <> NOTIFY_SKIP) then
    begin
      RemoveTelling(Index);
      Exit;
    end;
  // Whether the removal squeezes the holes out is known beforehand (more than MAX_IDLE_HOLES
  // holes, and more holes than entries, once this one is removed); the keys are checked before
  // anything changes, so that a changed key leaves the dictionary as it was (3.2, 7.5). The
  // entry being removed was found, so it goes even if its own key changed.
  Holes := FUsed - FCount;
  Squeeze := (Holes >= MAX_IDLE_HOLES) and (Holes + 2 > FCount);
  if Squeeze then
    CheckAll(Index);
  B := FHashes[Index] and FMask;
  Next := FNext[Index];
  I := FBuckets[B];
  if I = Index then
    FBuckets[B] := Next
  else
  begin
    while FNext[I] <> Index do
      I := FNext[I];
    FNext[I] := Next;
  end;
  FNext[Index] := HOLE;
  Dec(FCount);
  // NextVersion, written out
  if FVersion = MaxInt then
    FVersion := 0
  else
    FVersion := FVersion + 1;
  // Let go of the key and the value only once the table is whole again
  FKeyItems[Index] := Default(K);
  FValueItems[Index] := Default(V);
  if Squeeze then
    SqueezeHoles;
end;

procedure TDictionary<K, V>.RemoveTelling(Index: Integer);
var
  Removed: TPair<K, V>;
begin
  Removed := TPair<K, V>.Create(FKeyItems[Index], FValueItems[Index]);
  RemoveAt(Index, False);
  if Notifies then
    EntryRemoved(Removed.Key, Removed.Value);
end;

procedure TDictionary<K, V>.CheckAll(Skip: Integer);
var
  I: Integer;
begin
  if FByKey then
    if IsPlainKey(Default(K)) then
      Exit;
  for I := 0 to FUsed - 1 do
    if (I <> Skip) and (FNext[I] <> HOLE) then
      CheckEntry(I);
end;

procedure TDictionary<K, V>.Rehash(NewCapacity: Integer);
begin
  // Check every key first, so that the dictionary is left as it was when one has changed (7.5)
  CheckAll(-1);
  Place(NewCapacity);
end;

procedure TDictionary<K, V>.MoveLiveToFront;
var
  I, J: Integer;
begin
  J := 0;
  for I := 0 to FUsed - 1 do
    if FNext[I] <> HOLE then
    begin
      if J < I then
      begin
        FKeyItems[J] := FKeyItems[I];
        FValueItems[J] := FValueItems[I];
        FHashes[J] := FHashes[I];
        // Not a hole; the chains are made by the caller
        FNext[J] := -1;
      end;
      Inc(J);
    end;
  // Let go of the keys and values left behind in the entries that are no longer in use (a hole's
  // were let go of when it was removed)
  for I := J to FUsed - 1 do
  begin
    FKeyItems[I] := Default(K);
    FValueItems[I] := Default(V);
  end;
  FUsed := J;
end;

procedure TDictionary<K, V>.Place(NewCapacity: Integer);
var
  N: Integer;
begin
  // The live entries go to the front, then each array keeps its first FUsed entries in the new
  // capacity (NewCapacity is at least FCount), and the chains are made again
  if FUsed > FCount then
    MoveLiveToFront;
  SetLength(FKeyItems, NewCapacity);
  SetLength(FValueItems, NewCapacity);
  SetLength(FHashes, NewCapacity);
  SetLength(FNext, NewCapacity);
  N := 0;
  if NewCapacity > 0 then
  begin
    N := 1;
    while N < NewCapacity do
      N := 2 * N;
  end;
  SetLength(FBuckets, N);
  FMask := N - 1;
  FCapacity := NewCapacity;
  HashRelink(FBuckets, FHashes, FNext, FUsed);
end;

procedure TDictionary<K, V>.SqueezeHoles;
var
  I, B: Integer;
begin
  // Only the buckets of the live entries can be in use (the holes are out of the chains); they
  // are emptied and the chains made again here rather than by HashRelink, which would empty
  // every bucket (a time proportional to the capacity)
  for I := 0 to FUsed - 1 do
    if FNext[I] <> HOLE then
      FBuckets[Integer(FHashes[I] and FMask)] := -1;
  MoveLiveToFront;
  for I := 0 to FUsed - 1 do
  begin
    B := Integer(FHashes[I] and FMask);
    FNext[I] := FBuckets[B];
    FBuckets[B] := I;
  end;
end;

procedure TDictionary<K, V>.Add(const Key: K; const Value: V);
var
  Hash: Int64;
  I: Integer;
begin
  Hash := Find(Key, I);
  if I >= 0 then
    raise EListError.Create('duplicate key in `' + OwnerName + '`');
  if (FKeyNotify <> NOTIFY_SKIP) or (FValueNotify <> NOTIFY_SKIP) then
    EntryAdding(Key, Value);
  Append(Key, Value, Hash);
end;

function TDictionary<K, V>.TryAdd(const Key: K; const Value: V): Boolean;
var
  Hash: Int64;
  I: Integer;
begin
  Hash := Find(Key, I);
  Result := I < 0;
  if Result then
  begin
    if (FKeyNotify <> NOTIFY_SKIP) or (FValueNotify <> NOTIFY_SKIP) then
      EntryAdding(Key, Value);
    Append(Key, Value, Hash);
  end;
end;

procedure TDictionary<K, V>.AddOrSetValue(const Key: K; const Value: V);
var
  Hash: Int64;
  I: Integer;
  Kept: K;
  Old: V;
begin
  Hash := Find(Key, I);
  if (FKeyNotify <> NOTIFY_SKIP) or (FValueNotify <> NOTIFY_SKIP) then
    EntryAdding(Key, Value);
  if I < 0 then
  begin
    Append(Key, Value, Hash);
    Exit;
  end;
  // The key already there stays (9.3)
  if not Notifies then
  begin
    FValueItems[I] := Value;
    Exit;
  end;
  Kept := FKeyItems[I];
  Old := FValueItems[I];
  FValueItems[I] := Value;
  EntryReplaced(Kept, Key, Old, Value);
end;

// TryRemove without the result, written out so that it is small enough to be inlined.
procedure TDictionary<K, V>.Remove(const Key: K);
var
  I: Integer;
begin
  // An empty dictionary asks for no hash
  if FCount = 0 then
    Exit;
  I := IndexOfKey(Key);
  if I >= 0 then
    RemoveAt(I, True);
end;

function TDictionary<K, V>.TryRemove(const Key: K): Boolean;
var
  I: Integer;
begin
  // An empty dictionary asks for no hash
  if FCount = 0 then
    Exit(False);
  I := IndexOfKey(Key);
  Result := I >= 0;
  if Result then
    RemoveAt(I, True);
end;

function TDictionary<K, V>.ExtractPair(const Key: K): TPair<K, V>;
var
  I: Integer;
begin
  I := -1;
  if FCount > 0 then
    I := IndexOfKey(Key);
  if I < 0 then
    Exit(TPair<K, V>.Create(Key, Default(V)));
  Result := TPair<K, V>.Create(FKeyItems[I], FValueItems[I]);
  RemoveAt(I, False);
end;

procedure TDictionary<K, V>.Clear;
begin
  RemoveAll(True);
end;

procedure TDictionary<K, V>.RemoveAll(KeepCapacity: Boolean);
var
  N, I, J, Used, Mask: Integer;
  Removed: TArray<TPair<K, V>>;
  OldKeys: TArray<K>;
  OldValues: TArray<V>;
begin
  // No hash is asked for, so that a dictionary whose keys changed can always be emptied (7.5).
  // The entries leave first, then KeyNotify and ValueNotify hear of them in insertion order
  // (9.4).
  Removed := nil;
  if Notifies then
  begin
    SetLength(Removed, FCount);
    J := 0;
    for I := 0 to FUsed - 1 do
      if FNext[I] <> HOLE then
      begin
        Removed[J] := TPair<K, V>.Create(FKeyItems[I], FValueItems[I]);
        Inc(J);
      end;
  end;
  // The dictionary holds no entries while they are let go of, so that a destructor that looks at
  // it sees it empty (and a lookup finds every bucket empty: they are emptied first). The keys
  // and values are let go of entry by entry, in insertion order, from the arrays taken out of
  // the dictionary, so that an entry a destructor adds goes into arrays of their own and stays
  // (collections.md 9.4).
  Used := FUsed;
  FUsed := 0;
  FCount := 0;
  FVersion := NextVersion(FVersion);
  N := FCapacity;
  Mask := FMask;
  if KeepCapacity then
  begin
    for I := 0 to FMask do
      FBuckets[I] := -1;
  end
  else
  begin
    FBuckets := nil;
    FMask := -1;
  end;
  OldKeys := FKeyItems;
  OldValues := FValueItems;
  FKeyItems := nil;
  FValueItems := nil;
  FHashes := nil;
  FNext := nil;
  FCapacity := 0;
  for I := 0 to Used - 1 do
  begin
    OldKeys[I] := Default(K);
    OldValues[I] := Default(V);
  end;
  if KeepCapacity then
    if (FCapacity = 0) and (FMask = Mask) then
    begin
      // Nothing was added or reshaped while the entries were let go of: the buckets are still
      // empty and of the right size
      SetLength(FKeyItems, N);
      SetLength(FValueItems, N);
      SetLength(FHashes, N);
      SetLength(FNext, N);
      FCapacity := N;
    end
    else if FCapacity < N then
      // A destructor added entries: they are kept, and the capacity grows back to N
      Place(N);
  ItemsRemoved(Removed, 0);
end;

function TDictionary<K, V>.TryGetValue(const Key: K; out Value: V): Boolean;
var
  I: Integer;
begin
  I := IndexOfKey(Key);
  Result := I >= 0;
  if Result then
    Value := FValueItems[I]
  else
    Value := Default(V);
end;

function TDictionary<K, V>.ContainsKey(const Key: K): Boolean;
begin
  Result := IndexOfKey(Key) >= 0;
end;

function TDictionary<K, V>.ContainsValue(const Value: V): Boolean;
var
  I: Integer;
begin
  for I := 0 to FUsed - 1 do
    if (FNext[I] <> HOLE) and KeyEquals<V>(FValueItems[I], Value) then
      Exit(True);
  Result := False;
end;

function TDictionary<K, V>.GetItem(const Key: K): V;
var
  I: Integer;
begin
  I := IndexOfKey(Key);
  if I < 0 then
    raise EKeyNotFound.Create('key not found in `' + OwnerName + '`');
  Result := FValueItems[I];
end;

procedure TDictionary<K, V>.SetItem(const Key: K; const Value: V);
begin
  AddOrSetValue(Key, Value);
end;

function TDictionary<K, V>.GetCapacity: Integer;
begin
  Result := FCapacity;
end;

procedure TDictionary<K, V>.SetCapacity(Value: Integer);
begin
  if Value < FCount then
    raise CapacityError(FKind, Value, FCount);
  if Value <> FCapacity then
  begin
    Rehash(Value);
    FVersion := NextVersion(FVersion);
  end;
end;

procedure TDictionary<K, V>.TrimExcess;
begin
  SetCapacity(FCount);
end;

function TDictionary<K, V>.ToArray: TArray<TPair<K, V>>;
var
  I, J: Integer;
begin
  SetLength(Result, FCount);
  J := 0;
  for I := 0 to FUsed - 1 do
    if FNext[I] <> HOLE then
    begin
      {$IFDEF DEBUG}
      CheckEntry(I);
      {$ENDIF}
      Result[J] := TPair<K, V>.Create(FKeyItems[I], FValueItems[I]);
      Inc(J);
    end;
end;

function TDictionary<K, V>.GetEnumerator: TPairEnumerator;
begin
  Result := Default(TPairEnumerator);
  Result.FDict := Self;
  Result.FIndex := -1;
  Result.FVersion := FVersion;
end;

function TDictionary<K, V>.GetKeys: TKeyCollection;
begin
  if FKeys = nil then
    FKeys := TKeyCollection.Create(Self);
  Result := FKeys;
end;

function TDictionary<K, V>.GetValues: TValueCollection;
begin
  if FValues = nil then
    FValues := TValueCollection.Create(Self);
  Result := FValues;
end;

function TDictionary<K, V>.KeyArray: TArray<K>;
var
  I, J: Integer;
begin
  SetLength(Result, FCount);
  J := 0;
  for I := 0 to FUsed - 1 do
    if FNext[I] <> HOLE then
    begin
      {$IFDEF DEBUG}
      CheckEntry(I);
      {$ENDIF}
      Result[J] := FKeyItems[I];
      Inc(J);
    end;
end;

function TDictionary<K, V>.TPairEnumerator.GetCurrent: TPair<K, V>;
begin
  Result := TPair<K, V>.Create(FDict.FKeyItems[FIndex], FDict.FValueItems[FIndex]);
end;

function TDictionary<K, V>.TPairEnumerator.MoveNext: Boolean;
begin
  FDict.CheckVersion(FVersion);
  FIndex := FDict.NextLive(FIndex + 1);
  Result := FIndex < FDict.FUsed;
  {$IFDEF DEBUG}
  if Result then
    FDict.CheckEntry(FIndex);
  {$ENDIF}
end;

function TDictionary<K, V>.TKeyEnumerator.GetCurrent: K;
begin
  Result := FDict.FKeyItems[FIndex];
end;

function TDictionary<K, V>.TKeyEnumerator.MoveNext: Boolean;
begin
  FDict.CheckVersion(FVersion);
  FIndex := FDict.NextLive(FIndex + 1);
  Result := FIndex < FDict.FUsed;
  {$IFDEF DEBUG}
  if Result then
    FDict.CheckEntry(FIndex);
  {$ENDIF}
end;

function TDictionary<K, V>.TValueEnumerator.GetCurrent: V;
begin
  Result := FDict.FValueItems[FIndex];
end;

function TDictionary<K, V>.TValueEnumerator.MoveNext: Boolean;
begin
  FDict.CheckVersion(FVersion);
  FIndex := FDict.NextLive(FIndex + 1);
  Result := FIndex < FDict.FUsed;
  {$IFDEF DEBUG}
  if Result then
    FDict.CheckEntry(FIndex);
  {$ENDIF}
end;

constructor TDictionary<K, V>.TKeyCollection.Create(ADict: TDictionary<K, V>);
begin
  inherited Create;
  FDict := ADict;
end;

destructor TDictionary<K, V>.TKeyCollection.Destroy;
begin
  // Freed by mistake: the dictionary makes a new one the next time Keys is read (7.3)
  if FDict.FKeys = Self then
    FDict.FKeys := nil;
  inherited Destroy;
end;

function TDictionary<K, V>.TKeyCollection.GetCount: Integer;
begin
  Result := FDict.FCount;
end;

function TDictionary<K, V>.TKeyCollection.ToArray: TArray<K>;
begin
  Result := FDict.KeyArray;
end;

function TDictionary<K, V>.TKeyCollection.GetEnumerator: TKeyEnumerator;
begin
  Result := Default(TKeyEnumerator);
  Result.FDict := FDict;
  Result.FIndex := -1;
  Result.FVersion := FDict.FVersion;
end;

constructor TDictionary<K, V>.TValueCollection.Create(ADict: TDictionary<K, V>);
begin
  inherited Create;
  FDict := ADict;
end;

destructor TDictionary<K, V>.TValueCollection.Destroy;
begin
  if FDict.FValues = Self then
    FDict.FValues := nil;
  inherited Destroy;
end;

function TDictionary<K, V>.TValueCollection.GetCount: Integer;
begin
  Result := FDict.FCount;
end;

function TDictionary<K, V>.TValueCollection.ToArray: TArray<V>;
var
  I, J: Integer;
begin
  SetLength(Result, FDict.FCount);
  J := 0;
  for I := 0 to FDict.FUsed - 1 do
    if FDict.FNext[I] <> HOLE then
    begin
      {$IFDEF DEBUG}
      FDict.CheckEntry(I);
      {$ENDIF}
      Result[J] := FDict.FValueItems[I];
      Inc(J);
    end;
end;

function TDictionary<K, V>.TValueCollection.GetEnumerator: TValueEnumerator;
begin
  Result := Default(TValueEnumerator);
  Result.FDict := FDict;
  Result.FIndex := -1;
  Result.FVersion := FDict.FVersion;
end;

// ===== THashSet<T> (chapter 8) =====

procedure THashSet<T>.Init(const AComparer: IEqualityComparer<T>);
begin
  FTable := TTable.Create(AComparer);
  FTable.FOwner := Self;
  FTable.FKind := 'set';
end;

constructor THashSet<T>.Create;
begin
  inherited Create;
  Init(nil);
end;

constructor THashSet<T>.Create(const AComparer: IEqualityComparer<T>);
begin
  inherited Create;
  Init(AComparer);
end;

constructor THashSet<T>.Create(const Values: array of T);
begin
  inherited Create;
  Init(nil);
  UnionWith(Values);
end;

constructor THashSet<T>.Create(const Source: TEnumerable<T>);
begin
  inherited Create;
  Init(nil);
  UnionWith(Source);
end;

destructor THashSet<T>.Destroy;
begin
  FTable.Free;
  inherited Destroy;
end;

function THashSet<T>.GetCount: Integer;
begin
  Result := FTable.FCount;
end;

function THashSet<T>.GetCapacity: Integer;
begin
  Result := FTable.GetCapacity;
end;

procedure THashSet<T>.SetCapacity(Value: Integer);
begin
  FTable.SetCapacity(Value);
end;

function THashSet<T>.Add(const Value: T): Boolean;
begin
  Result := FTable.TryAdd(Value, False);
end;

function THashSet<T>.Remove(const Value: T): Boolean;
begin
  Result := FTable.TryRemove(Value);
end;

function THashSet<T>.Contains(const Value: T): Boolean;
begin
  Result := FTable.ContainsKey(Value);
end;

procedure THashSet<T>.UnionWith(const Source: TEnumerable<T>);
begin
  UnionWith(Source.ToArray);
end;

procedure THashSet<T>.UnionWith(const Values: array of T);
var
  I: Integer;
begin
  // The items are checked first: an item changed after it was added would otherwise come to light
  // only when growing places the items again, part way through (3.2, 7.5)
  FTable.CheckAll(-1);
  // The items not in the set yet go to the end, in the order of Values
  for I := 0 to High(Values) do
    FTable.TryAdd(Values[I], False);
end;

procedure THashSet<T>.IntersectWith(const Source: TEnumerable<T>);
begin
  IntersectWith(Source.ToArray);
end;

procedure THashSet<T>.IntersectWith(const Values: array of T);
var
  Keep: TTable;
  Items: TArray<T>;
  I: Integer;
begin
  // Values is put in a table of its own, compared the same way as the set, so that each item
  // of the set is looked up once. The items kept stay in their order.
  // Checked first, as in UnionWith: removing may squeeze the holes out part way through
  FTable.CheckAll(-1);
  Keep := TTable.Create(FTable.FComparer);
  try
    for I := 0 to High(Values) do
      Keep.TryAdd(Values[I], False);
    Items := FTable.KeyArray;
    for I := 0 to High(Items) do
      if not Keep.ContainsKey(Items[I]) then
        Remove(Items[I]);
  finally
    Keep.Free;
  end;
end;

procedure THashSet<T>.ExceptWith(const Source: TEnumerable<T>);
begin
  ExceptWith(Source.ToArray);
end;

procedure THashSet<T>.ExceptWith(const Values: array of T);
var
  I: Integer;
begin
  // Checked first, as in UnionWith: removing may squeeze the holes out part way through
  FTable.CheckAll(-1);
  for I := 0 to High(Values) do
    Remove(Values[I]);
end;

procedure THashSet<T>.Clear;
begin
  FTable.Clear;
end;

procedure THashSet<T>.TrimExcess;
begin
  FTable.TrimExcess;
end;

function THashSet<T>.ToArray: TArray<T>;
begin
  Result := FTable.KeyArray;
end;

function THashSet<T>.GetEnumerator: TEnumerator;
begin
  Result := Default(TEnumerator);
  Result.FDict := FTable;
  Result.FIndex := -1;
  Result.FVersion := FTable.FVersion;
end;

// ===== Owning containers (chapter 9) =====

// The exception for an instance of a reference counted class put in an owning container (9.2).
function OwnRefCountedError(const ClassName: string): EInvalidOperation;
begin
  Result := EInvalidOperation.Create('cannot own a reference counted object of class `' +
    ClassName + '`');
end;

// Raises when one of Items cannot be owned (OwnsObjects set to True; 9.2).
procedure CheckOwnable<T: class>(const Items: TArray<T>);
var
  I: Integer;
begin
  for I := 0 to High(Items) do
    if IsRefCountedObject(Items[I]) then
      raise OwnRefCountedError(Items[I].ClassName);
end;

constructor TObjectList<T>.Create;
begin
  FOwnsObjects := True;
  inherited Create;
end;

constructor TObjectList<T>.Create(AOwnsObjects: Boolean);
begin
  FOwnsObjects := AOwnsObjects;
  inherited Create;
end;

constructor TObjectList<T>.Create(const AComparer: IComparer<T>);
begin
  FOwnsObjects := True;
  inherited Create(AComparer);
end;

constructor TObjectList<T>.Create(const AComparer: IComparer<T>; AOwnsObjects: Boolean);
begin
  FOwnsObjects := AOwnsObjects;
  inherited Create(AComparer);
end;

constructor TObjectList<T>.Create(const Values: array of T);
begin
  FOwnsObjects := True;
  inherited Create(Values);
end;

// OwnsObjects is set first: the items are checked as they go in (9.2).
constructor TObjectList<T>.Create(const Values: array of T; AOwnsObjects: Boolean);
begin
  FOwnsObjects := AOwnsObjects;
  inherited Create(Values);
end;

constructor TObjectList<T>.Create(const Source: TEnumerable<T>);
begin
  FOwnsObjects := True;
  inherited Create(Source);
end;

constructor TObjectList<T>.Create(const Source: TEnumerable<T>; AOwnsObjects: Boolean);
begin
  FOwnsObjects := AOwnsObjects;
  inherited Create(Source);
end;

procedure TObjectList<T>.SetOwnsObjects(Value: Boolean);
begin
  if Value and not FOwnsObjects then
    CheckOwnable<T>(ToArray);
  FOwnsObjects := Value;
end;

procedure TObjectList<T>.Notify(const Item: T; Action: TCollectionNotification);
begin
  if not FOwnsObjects then
    Exit;
  if Action = TCollectionNotification.cnRemoved then
    Item.Free
  else if IsRefCountedObject(Item) then
    raise OwnRefCountedError(Item.ClassName);
end;

constructor TObjectStack<T>.Create;
begin
  FOwnsObjects := True;
  inherited Create;
end;

constructor TObjectStack<T>.Create(AOwnsObjects: Boolean);
begin
  FOwnsObjects := AOwnsObjects;
  inherited Create;
end;

constructor TObjectStack<T>.Create(const Values: array of T);
begin
  FOwnsObjects := True;
  inherited Create(Values);
end;

// OwnsObjects is set first: the items are checked as they go in (9.2).
constructor TObjectStack<T>.Create(const Values: array of T; AOwnsObjects: Boolean);
begin
  FOwnsObjects := AOwnsObjects;
  inherited Create(Values);
end;

constructor TObjectStack<T>.Create(const Source: TEnumerable<T>);
begin
  FOwnsObjects := True;
  inherited Create(Source);
end;

constructor TObjectStack<T>.Create(const Source: TEnumerable<T>; AOwnsObjects: Boolean);
begin
  FOwnsObjects := AOwnsObjects;
  inherited Create(Source);
end;

procedure TObjectStack<T>.SetOwnsObjects(Value: Boolean);
begin
  if Value and not FOwnsObjects then
    CheckOwnable<T>(ToArray);
  FOwnsObjects := Value;
end;

procedure TObjectStack<T>.Notify(const Item: T; Action: TCollectionNotification);
begin
  if not FOwnsObjects then
    Exit;
  if Action = TCollectionNotification.cnRemoved then
    Item.Free
  else if IsRefCountedObject(Item) then
    raise OwnRefCountedError(Item.ClassName);
end;

procedure TObjectStack<T>.Pop;
begin
  Notify(Extract, TCollectionNotification.cnRemoved);
end;

constructor TObjectQueue<T>.Create;
begin
  FOwnsObjects := True;
  inherited Create;
end;

constructor TObjectQueue<T>.Create(AOwnsObjects: Boolean);
begin
  FOwnsObjects := AOwnsObjects;
  inherited Create;
end;

constructor TObjectQueue<T>.Create(const Values: array of T);
begin
  FOwnsObjects := True;
  inherited Create(Values);
end;

// OwnsObjects is set first: the items are checked as they go in (9.2).
constructor TObjectQueue<T>.Create(const Values: array of T; AOwnsObjects: Boolean);
begin
  FOwnsObjects := AOwnsObjects;
  inherited Create(Values);
end;

constructor TObjectQueue<T>.Create(const Source: TEnumerable<T>);
begin
  FOwnsObjects := True;
  inherited Create(Source);
end;

constructor TObjectQueue<T>.Create(const Source: TEnumerable<T>; AOwnsObjects: Boolean);
begin
  FOwnsObjects := AOwnsObjects;
  inherited Create(Source);
end;

procedure TObjectQueue<T>.SetOwnsObjects(Value: Boolean);
begin
  if Value and not FOwnsObjects then
    CheckOwnable<T>(ToArray);
  FOwnsObjects := Value;
end;

procedure TObjectQueue<T>.Notify(const Item: T; Action: TCollectionNotification);
begin
  if not FOwnsObjects then
    Exit;
  if Action = TCollectionNotification.cnRemoved then
    Item.Free
  else if IsRefCountedObject(Item) then
    raise OwnRefCountedError(Item.ClassName);
end;

procedure TObjectQueue<T>.Dequeue;
begin
  Notify(Extract, TCollectionNotification.cnRemoved);
end;

constructor TObjectDictionary<K, V>.Create(Ownerships: TDictionaryOwnerships);
begin
  SetOwnerships(Ownerships);
  inherited Create;
end;

constructor TObjectDictionary<K, V>.Create(Ownerships: TDictionaryOwnerships;
  ACapacity: Integer);
begin
  SetOwnerships(Ownerships);
  inherited Create(ACapacity);
end;

constructor TObjectDictionary<K, V>.Create(Ownerships: TDictionaryOwnerships;
  const AComparer: IEqualityComparer<K>);
begin
  SetOwnerships(Ownerships);
  inherited Create(AComparer);
end;

constructor TObjectDictionary<K, V>.Create(Ownerships: TDictionaryOwnerships;
  ACapacity: Integer; const AComparer: IEqualityComparer<K>);
begin
  SetOwnerships(Ownerships);
  inherited Create(ACapacity, AComparer);
end;

// Only a class can be owned (9.3); a constant Ownerships is checked by the compiler (E0768).
procedure TObjectDictionary<K, V>.SetOwnerships(Value: TDictionaryOwnerships);
begin
  if TDictionaryOwnership.doOwnsKeys in Value then
    CheckClassType(Default(K), '`doOwnsKeys` needs a class type for the keys');
  if TDictionaryOwnership.doOwnsValues in Value then
    CheckClassType(Default(V), '`doOwnsValues` needs a class type for the values');
  FOwnerships := Value;
end;

procedure TObjectDictionary<K, V>.KeyNotify(const Key: K; Action: TCollectionNotification);
begin
  if not (TDictionaryOwnership.doOwnsKeys in FOwnerships) then
    Exit;
  if Action = TCollectionNotification.cnRemoved then
    FreeIfClass(Key)
  else if IsRefCountedObject(Key) then
    raise OwnRefCountedError(ObjectClassName(Key));
end;

procedure TObjectDictionary<K, V>.ValueNotify(const Value: V; Action: TCollectionNotification);
begin
  if not (TDictionaryOwnership.doOwnsValues in FOwnerships) then
    Exit;
  if Action = TCollectionNotification.cnRemoved then
    FreeIfClass(Value)
  else if IsRefCountedObject(Value) then
    raise OwnRefCountedError(ObjectClassName(Value));
end;

// ===== TArray (10.4) =====

class function TArray.ByDefaultOrder<T>(const Comparer: IComparer<T>): Boolean;
begin
  Result := Comparer = nil;
  if Result then
    CheckDefaultOrder(Default(T));
end;

class procedure TArray.SortItems<T>(var Items: TArray<T>; const Comparer: IComparer<T>);
const
  // The length of the runs sorted by insertion before merging.
  RunLength = 8;
var
  N, Lo, Mid, Hi, Width, I, J, L, R, C: Integer;
  Src, Dst, Swap: TArray<T>;
  A, B: T;
  ByKey: Boolean;
begin
  // The elements of a type ordered by a built-in comparison are sorted by the VM, the same way.
  if IsPlainOrder(Default(T)) then
    if Comparer = nil then
    begin
      ArraySortRange(Items, 0, Length(Items));
      Exit;
    end;
  // Each comparison below is `KeyCompare<T>(A, B)` when ByKey, else `Comparer.Compare(A, B)`.
  // The loops are written once per kind so that the kind is not tested at every comparison.
  ByKey := ByDefaultOrder<T>(Comparer);
  // The sort works on local arrays (reading a `var` parameter costs one more load per element).
  // Items is cleared so that it never holds an array in the middle of a pass; the callers throw
  // it away when the comparer raises.
  Src := Items;
  Items := nil;
  N := Length(Src);
  Lo := 0;
  while Lo < N do
  begin
    if N - Lo > RunLength then
      Hi := Lo + RunLength
    else
      Hi := N;
    for I := Lo + 1 to Hi - 1 do
    begin
      B := Src[I];
      J := I - 1;
      if ByKey then
        while J >= Lo do
        begin
          A := Src[J];
          if KeyCompare<T>(A, B) <= 0 then
            Break;
          Src[J + 1] := A;
          Dec(J);
        end
      else
        while J >= Lo do
        begin
          A := Src[J];
          if Comparer.Compare(A, B) <= 0 then
            Break;
          Src[J + 1] := A;
          Dec(J);
        end;
      Src[J + 1] := B;
    end;
    Lo := Hi;
  end;
  if N <= RunLength then
  begin
    Items := Src;
    Exit;
  end;
  // Merge pairs of sorted runs from Src into Dst, doubling the width each pass.
  SetLength(Dst, N);
  Width := RunLength;
  while Width < N do
  begin
    Lo := 0;
    while Lo < N do
    begin
      if N - Lo > Width then
        Mid := Lo + Width
      else
        Mid := N;
      if N - Mid > Width then
        Hi := Mid + Width
      else
        Hi := N;
      L := Lo;
      R := Mid;
      I := Lo;
      if Mid < Hi then
      begin
        // Already in order (one comparison, so sorted input is linear): the runs are copied as
        // they are, the left one first.
        A := Src[Mid - 1];
        B := Src[Mid];
        if ByKey then
          C := KeyCompare<T>(A, B)
        else
          C := Comparer.Compare(A, B);
        if C > 0 then
        begin
          // A and B hold Src[L] and Src[R]; only the side that moved is read again. Ties take
          // the left run first, which keeps the sort stable. The loop ends when one run is used
          // up, and the copies below take the rest of the other.
          A := Src[L];
          if ByKey then
            while True do
            begin
              if KeyCompare<T>(A, B) <= 0 then
              begin
                Dst[I] := A;
                Inc(I);
                Inc(L);
                if L = Mid then
                  Break;
                A := Src[L];
              end
              else
              begin
                Dst[I] := B;
                Inc(I);
                Inc(R);
                if R = Hi then
                  Break;
                B := Src[R];
              end;
            end
          else
            while True do
            begin
              if Comparer.Compare(A, B) <= 0 then
              begin
                Dst[I] := A;
                Inc(I);
                Inc(L);
                if L = Mid then
                  Break;
                A := Src[L];
              end
              else
              begin
                Dst[I] := B;
                Inc(I);
                Inc(R);
                if R = Hi then
                  Break;
                B := Src[R];
              end;
            end;
        end;
      end;
      while L < Mid do
      begin
        Dst[I] := Src[L];
        Inc(L);
        Inc(I);
      end;
      while R < Hi do
      begin
        Dst[I] := Src[R];
        Inc(R);
        Inc(I);
      end;
      Lo := Hi;
    end;
    Swap := Src;
    Src := Dst;
    Dst := Swap;
    if Width > N div 2 then
      Width := N
    else
      Width := Width * 2;
  end;
  Items := Src;
end;

class function TArray.SearchItems<T>(const Values: array of T; Count: Integer; const Item: T;
  out FoundIndex: Integer; const Comparer: IComparer<T>): Boolean;
var
  Lo, Hi, Mid, C: Integer;
  ByKey: Boolean;
begin
  // Each comparison below is `KeyCompare<T>(A, B)` when ByKey, else `Comparer.Compare(A, B)`.
  ByKey := ByDefaultOrder<T>(Comparer);
  Lo := 0;
  Hi := Count;
  while Lo < Hi do
  begin
    Mid := Lo + (Hi - Lo) div 2;
    if ByKey then
      C := KeyCompare<T>(Values[Mid], Item)
    else
      C := Comparer.Compare(Values[Mid], Item);
    if C < 0 then
      Lo := Mid + 1
    else
      Hi := Mid;
  end;
  FoundIndex := Lo;
  Result := False;
  if Lo < Count then
  begin
    if ByKey then
      C := KeyCompare<T>(Values[Lo], Item)
    else
      C := Comparer.Compare(Values[Lo], Item);
    Result := C = 0;
  end;
end;

class procedure TArray.Sort<T>(var Values: array of T);
begin
  Sort<T>(Values, nil, 0, Length(Values));
end;

class procedure TArray.Sort<T>(var Values: array of T; const Comparer: IComparer<T>);
begin
  Sort<T>(Values, Comparer, 0, Length(Values));
end;

// Sorts a copy of the range and writes it back once sorted, so that a comparer that raises leaves
// Values as it was (10.3).
class procedure TArray.Sort<T>(var Values: array of T; const Comparer: IComparer<T>;
  Index, Count: Integer);
var
  Work: TArray<T>;
  I: Integer;
begin
  // The same checks and messages as the range of ArrayMoveRange (bytecode.md 2.22).
  if Count < 0 then
    raise ERangeError.Create('array count ' + IntToStr(Count) + ' is out of range');
  if (Index < 0) or (Index > Length(Values) - Count) then
    raise ERangeError.Create('array range ' + IntToStr(Index) + '..' +
      IntToStr(Int64(Index) + Count - 1) + ' is out of range 0..' + IntToStr(High(Values)));
  SetLength(Work, Count);
  for I := 0 to Count - 1 do
    Work[I] := Values[Index + I];
  SortItems<T>(Work, Comparer);
  for I := 0 to Count - 1 do
    Values[Index + I] := Work[I];
end;

class function TArray.BinarySearch<T>(const Values: array of T; const Item: T;
  out FoundIndex: Integer): Boolean;
begin
  Result := SearchItems<T>(Values, Length(Values), Item, FoundIndex, nil);
end;

class function TArray.BinarySearch<T>(const Values: array of T; const Item: T;
  out FoundIndex: Integer; const Comparer: IComparer<T>): Boolean;
begin
  Result := SearchItems<T>(Values, Length(Values), Item, FoundIndex, Comparer);
end;

end.
