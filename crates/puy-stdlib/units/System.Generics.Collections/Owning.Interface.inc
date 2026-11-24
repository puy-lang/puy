  // ===== Owning containers (chapter 9) =====
type
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
