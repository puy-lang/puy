type
  // How a sorted TStringList treats a string it already has (docs/spec/stdlib/System.Classes/
  // TStringList.md 1): dupIgnore leaves it out, dupAccept adds it, dupError raises.
  TDuplicates = (dupIgnore, dupAccept, dupError);

  // An operation a TStringList does not allow in its state: inserting into, reordering or
  // writing a string of a sorted list, or a duplicate in a sorted list with dupError.
  EStringListError = class(Exception);

  // The event of OnChange / OnChanging.
  TNotifyEvent = procedure(Sender: TObject) of object;

  TStrings = class;

  // Walks a TStrings in index order (for..in). It reads Count at every step, so it does not
  // notice a change other than a different count.
  TStringsEnumerator = record
  private
    FStrings: TStrings;
    FIndex: Integer;
    function GetCurrent: string;
  public
    function MoveNext: Boolean;
    property Current: string read GetCurrent;
  end;

  // A sequence of strings, each with an object (TStringList.md 2). A descendant overrides Get,
  // GetCount, Insert, Delete and Clear (and GetObject / PutObject to keep objects); the rest is
  // written on them.
  TStrings = class
  private
    FDelimiter: Char;
    FQuoteChar: Char;
    FNameValueSeparator: Char;
    FStrictDelimiter: Boolean;
    FLineBreak: string;
    FUpdateCount: Integer;
    // The encoding of the last LoadFromStream; a copy owned by the list when it is not one of
    // the standard encodings of TEncoding.
    FEncoding: TEncoding;
    FOwnsEncoding: Boolean;
    function GetCommaText: string;
    procedure SetCommaText(const Value: string);
    function GetDelimitedText: string;
    procedure SetDelimitedText(const Value: string);
    function GetName(Index: Integer): string;
    function GetValue(const Name: string): string;
    procedure SetValue(const Name, Value: string);
    function GetValueFromIndex(Index: Integer): string;
    procedure SetValueFromIndex(Index: Integer; const Value: string);
    function GetRange(const Range: TIndexRange): TArray<string>;
    procedure SetEncoding(Value: TEncoding);
  protected
    function Get(Index: Integer): string; virtual; abstract;
    function GetCount: Integer; virtual; abstract;
    function GetObject(Index: Integer): TObject; virtual;
    procedure Put(Index: Integer; const S: string); virtual;
    procedure PutObject(Index: Integer; AObject: TObject); virtual;
    function GetCapacity: Integer; virtual;
    procedure SetCapacity(NewCapacity: Integer); virtual;
    function GetTextStr: string; virtual;
    procedure SetTextStr(const Value: string); virtual;
    // Called by the first BeginUpdate (True) and the last EndUpdate (False).
    procedure SetUpdateState(Updating: Boolean); virtual;
    // The order and the equality of IndexOf, IndexOfName and Values: AnsiCompareText here.
    function CompareStrings(const S1, S2: string): Integer; virtual;
    property UpdateCount: Integer read FUpdateCount;
  public
    constructor Create;
    destructor Destroy; override;
    function Add(const S: string): Integer; virtual;
    function AddObject(const S: string; AObject: TObject): Integer; virtual;
    // Adds Name, NameValueSeparator and Value as one string; returns the list.
    function AddPair(const Name, Value: string): TStrings; overload;
    function AddPair(const Name, Value: string; AObject: TObject): TStrings; overload;
    procedure Append(const S: string);
    procedure AddStrings(const Strings: TStrings); overload; virtual;
    procedure AddStrings(const Strings: array of string); overload;
    // The strings, the objects and the settings of TStrings (Delimiter, QuoteChar,
    // NameValueSeparator, StrictDelimiter, LineBreak) of Source.
    procedure Assign(const Source: TStrings); virtual;
    procedure BeginUpdate;
    procedure EndUpdate;
    procedure Clear; virtual; abstract;
    procedure Delete(Index: Integer); virtual; abstract;
    procedure Exchange(Index1, Index2: Integer); virtual;
    function GetEnumerator: TStringsEnumerator;
    function IndexOf(const S: string): Integer; virtual;
    function IndexOfName(const Name: string): Integer; virtual;
    function IndexOfObject(AObject: TObject): Integer; virtual;
    procedure Insert(Index: Integer; const S: string); virtual; abstract;
    procedure InsertObject(Index: Integer; const S: string; AObject: TObject); virtual;
    procedure LoadFromFile(const FileName: string); overload; virtual;
    procedure LoadFromFile(const FileName: string; Encoding: TEncoding); overload; virtual;
    procedure LoadFromStream(const Stream: TStream); overload; virtual;
    procedure LoadFromStream(const Stream: TStream; Encoding: TEncoding); overload; virtual;
    procedure Move(CurIndex, NewIndex: Integer); virtual;
    procedure SaveToFile(const FileName: string); overload; virtual;
    procedure SaveToFile(const FileName: string; Encoding: TEncoding); overload; virtual;
    procedure SaveToStream(const Stream: TStream); overload; virtual;
    procedure SaveToStream(const Stream: TStream; Encoding: TEncoding); overload; virtual;
    function ToStringArray: TArray<string>;
    function ToObjectArray: TArray<TObject>;
    property Capacity: Integer read GetCapacity write SetCapacity;
    property CommaText: string read GetCommaText write SetCommaText;
    property Count: Integer read GetCount;
    property Delimiter: Char read FDelimiter write FDelimiter;
    property DelimitedText: string read GetDelimitedText write SetDelimitedText;
    property Encoding: TEncoding read FEncoding;
    property LineBreak: string read FLineBreak write FLineBreak;
    property Names[Index: Integer]: string read GetName;
    property NameValueSeparator: Char read FNameValueSeparator write FNameValueSeparator;
    property Objects[Index: Integer]: TObject read GetObject write PutObject;
    property QuoteChar: Char read FQuoteChar write FQuoteChar;
    property StrictDelimiter: Boolean read FStrictDelimiter write FStrictDelimiter;
    property Strings[Index: Integer]: string read Get write Put; default;
    property Strings[const Range: TIndexRange]: TArray<string> read GetRange; default;
    property Text: string read GetTextStr write SetTextStr;
    property Values[const Name: string]: string read GetValue write SetValue;
    property ValueFromIndex[Index: Integer]: string read GetValueFromIndex
      write SetValueFromIndex;
  end;

  // One line of a TStringList: the string and its object.
  TStringItem = record
    FString: string;
    FObject: TObject;
  end;

  TStringList = class;

  // The order of CustomSort: negative when the string at Index1 goes before the one at Index2.
  TStringListSortCompare = function(List: TStringList; Index1, Index2: Integer): Integer;

  // The list of strings (TStringList.md 3): the lines are kept in a TList<TStringItem>.
  TStringList = class(TStrings)
  private
    FList: TList<TStringItem>;
    FSorted: Boolean;
    FDuplicates: TDuplicates;
    FCaseSensitive: Boolean;
    FOwnsObjects: Boolean;
    FOnChange: TNotifyEvent;
    FOnChanging: TNotifyEvent;
    procedure SetSorted(Value: Boolean);
    procedure SetCaseSensitive(Value: Boolean);
    procedure SetOwnsObjects(Value: Boolean);
    // Raises EInvalidOperation when AObject cannot be owned (OwnsObjects; TStringList.md 3.5).
    procedure CheckOwnable(AObject: TObject);
    procedure CheckIndex(Index, Max: Integer);
    procedure CheckUnsorted(const What: string);
    // Sorts with Compare whether or not the list is Sorted.
    procedure SortBy(Compare: TStringListSortCompare);
    // Frees Items[From..] in order; when one Free raises, the rest are still freed, and the
    // exceptions overlap.
    procedure FreeObjects(const Items: TArray<TStringItem>; From: Integer);
    procedure InsertItem(Index: Integer; const S: string; AObject: TObject);
  protected
    function Get(Index: Integer): string; override;
    function GetCount: Integer; override;
    function GetObject(Index: Integer): TObject; override;
    procedure Put(Index: Integer; const S: string); override;
    procedure PutObject(Index: Integer; AObject: TObject); override;
    function GetCapacity: Integer; override;
    procedure SetCapacity(NewCapacity: Integer); override;
    procedure SetUpdateState(Updating: Boolean); override;
    // OnChanging / OnChange, unless between BeginUpdate and EndUpdate.
    procedure Changing; virtual;
    procedure Changed; virtual;
    // CompareStr when CaseSensitive, else AnsiCompareText.
    function CompareStrings(const S1, S2: string): Integer; override;
  public
    constructor Create; overload;
    constructor Create(OwnsObjects: Boolean); overload;
    destructor Destroy; override;
    function Add(const S: string): Integer; override;
    function AddObject(const S: string; AObject: TObject): Integer; override;
    procedure Clear; override;
    procedure Delete(Index: Integer); override;
    procedure Exchange(Index1, Index2: Integer); override;
    // The binary search by CompareStrings: the smallest index of a string equal to S (True),
    // else the index S would be inserted at (False). The list is taken to be sorted.
    function Find(const S: string; var Index: Integer): Boolean; virtual;
    function IndexOf(const S: string): Integer; override;
    procedure Insert(Index: Integer; const S: string); override;
    procedure InsertObject(Index: Integer; const S: string; AObject: TObject); override;
    procedure Move(CurIndex, NewIndex: Integer); override;
    // Stable sorts by CompareStrings / Compare; nothing when Sorted.
    procedure Sort; virtual;
    procedure CustomSort(Compare: TStringListSortCompare); virtual;
    property Duplicates: TDuplicates read FDuplicates write FDuplicates;
    property Sorted: Boolean read FSorted write SetSorted;
    property CaseSensitive: Boolean read FCaseSensitive write SetCaseSensitive;
    property OwnsObjects: Boolean read FOwnsObjects write SetOwnsObjects;
    property OnChange: TNotifyEvent read FOnChange write FOnChange;
    property OnChanging: TNotifyEvent read FOnChanging write FOnChanging;
  end;
