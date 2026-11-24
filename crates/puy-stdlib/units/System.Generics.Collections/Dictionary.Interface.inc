  // ===== TDictionary<K, V> (chapter 7) =====
type
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
