program StdlibContainerRuleEdges;

// Lifetime rules of the containers not seen elsewhere (collections.md 3.2, 3.4, 7.3, 9.2, 9.5):
// a dictionary or a set freed while a for..in walks it (directly or through Keys) raises
// EAccessViolation at the next MoveNext, and so does any use of a freed container; nothing
// leaks. TObjectList<TObject> refuses a reference counted instance given through InsertRange or
// a TEnumerable<T> source of Create (no item is added; the refused Create destroys the half-made
// list and frees none of the items). An owning dictionary
// whose value destructor raises in ItemReplaced still frees the key it was handed, and whose
// Clear raises while it is freed still frees Keys / Values.

uses System.SysUtils, System.Generics.Collections;

type
  TRef = class(TRefObject)
  public
    Name: string;
    constructor Create(const AName: string);
    destructor Destroy; override;
  end;

  TItem = class
  public
    Name: string;
    Fail: Boolean;
    constructor Create(const AName: string; AFail: Boolean = False);
    destructor Destroy; override;
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

  EFail = class(Exception);

  TLogList = class(TObjectList<TObject>)
  public
    destructor Destroy; override;
  end;

constructor TRef.Create(const AName: string);
begin
  Name := AName;
end;

destructor TRef.Destroy;
begin
  WriteLn('destroy ', Name);
  inherited Destroy;
end;

constructor TItem.Create(const AName: string; AFail: Boolean);
begin
  Name := AName;
  Fail := AFail;
end;

destructor TItem.Destroy;
begin
  WriteLn('free ', Name);
  inherited Destroy;
  if Fail then
    raise EFail.Create('cannot free ' + Name);
end;

destructor TLogList.Destroy;
begin
  WriteLn('destroy the list');
  inherited Destroy;
end;

function TItem.Equals(Obj: TObject): Boolean;
begin
  Result := (Obj is TItem) and (TItem(Obj).Name[1] = Name[1]);
end;

function TItem.GetHashCode: Int64;
begin
  Result := Ord(Name[1]);
end;

// The instances made in the calls live until the end of this routine (design/lifetime.md 3.4),
// so they are filled in here for the dictionary's Free to be the last reference
procedure Fill(D: TDictionary<string, TRef>);
begin
  D.Add('a', TRef.Create('ref a'));
  D.Add('b', TRef.Create('ref b'));
end;

procedure FreeWhileWalking(Kind: Integer);
var
  D: TDictionary<string, TRef>;
  S: THashSet<string>;
  P: TPair<string, TRef>;
  K: string;
begin
  D := TDictionary<string, TRef>.Create;
  Fill(D);
  S := THashSet<string>.Create(['x', 'y']);
  try
    case Kind of
      0:
        for P in D do
        begin
          WriteLn('visit ', P.Key);
          D.Free;
        end;
      1:
        for K in D.Keys do
        begin
          WriteLn('visit key ', K);
          D.Free;
        end;
      2:
        for K in S do
        begin
          WriteLn('visit ', K);
          S.Free;
        end;
    end;
  except
    on E: EAccessViolation do WriteLn('EAccessViolation');
  end;
  if Kind = 2 then
    D.Free
  else
    S.Free;
end;

procedure UseFreed;
var
  L: TList<Integer>;
  D: TDictionary<Integer, Integer>;
  S: TStack<Integer>;
begin
  L := TList<Integer>.Create;
  L.Free;
  try
    L.Add(1);
  except
    on E: EAccessViolation do WriteLn('list: EAccessViolation');
  end;
  D := TDictionary<Integer, Integer>.Create;
  D.Free;
  try
    WriteLn(D.Count);
  except
    on E: EAccessViolation do WriteLn('dictionary: EAccessViolation');
  end;
  S := TStack<Integer>.Create;
  S.Free;
  try
    S.Push(1);
  except
    on E: EAccessViolation do WriteLn('stack: EAccessViolation');
  end;
end;

procedure RefusedThroughRanges;
var
  L: TObjectList<TObject>;
  Src: TList<TObject>;
  R: TRef;
begin
  R := TRef.Create('refused');
  Src := TList<TObject>.Create;
  Src.Add(TItem.Create('manual'));
  Src.Add(R);
  L := TObjectList<TObject>.Create;
  try
    L.InsertRange(0, [TObject(R)]);
  except
    on E: EInvalidOperation do WriteLn('InsertRange: ', E.Message, ', count ', L.Count);
  end;
  L.Free;
  try
    TLogList.Create(Src).Free;
  except
    on E: EInvalidOperation do WriteLn('Create(Source): ', E.Message);
  end;
  Src[0].Free;
  Src.Free;
end;

procedure ReplacedValueRaises;
var
  D: TObjectDictionary<TItem, TItem>;
  K1, K2: TItem;
begin
  D := TObjectDictionary<TItem, TItem>.Create([TDictionaryOwnership.doOwnsKeys,
    TDictionaryOwnership.doOwnsValues]);
  K1 := TItem.Create('k1');
  K2 := TItem.Create('k2');
  D.Add(K1, TItem.Create('old value', True));
  try
    D.AddOrSetValue(K2, TItem.Create('new value'));
  except
    on E: EFail do WriteLn('AddOrSetValue: ', E.Message);
  end;
  WriteLn('kept ', D.Count, ' ', D[K1].Name);
  D.Free;
end;

procedure ClearRaisesWhileFreed;
var
  D: TObjectDictionary<string, TItem>;
  Count: Integer;
begin
  D := TObjectDictionary<string, TItem>.Create([TDictionaryOwnership.doOwnsValues]);
  D.Add('a', TItem.Create('a value', True));
  Count := D.Keys.Count + D.Values.Count;
  WriteLn('views ', Count);
  try
    D.Free;
  except
    on E: EFail do WriteLn('Free: ', E.Message);
  end;
end;

begin
  FreeWhileWalking(0);
  FreeWhileWalking(1);
  FreeWhileWalking(2);
  UseFreed;
  RefusedThroughRanges;
  ReplacedValueRaises;
  ClearRaisesWhileFreed;
  WriteLn('end');
end.
