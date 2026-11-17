program StdlibOwningDictionaryFree;

// TObjectDictionary<K, V> frees the keys and values it owns (collections.md 9.3): Remove,
// Clear and freeing the dictionary free both sides of each entry (the key, then the value) in
// insertion order; AddOrSetValue and D[K] := V free the value written over unless it is the new
// one. Writing to a key already there keeps the key the dictionary holds and frees the key passed
// in when it is another instance (equal by Equals), but not when it is the same instance.
// ExtractPair hands the entry over, TryAdd of a key already there and a duplicate Add take
// nothing, and a side not owned is never freed. Calls through the parent type free the same way.
// When a destructor raises, the rest are still freed and the exceptions overlap into one.

uses System.SysUtils, System.Generics.Collections;

type
  TItem = class
  public
    Name: string;
    Fails: Boolean;
    constructor Create(const AName: string; AFails: Boolean = False);
    destructor Destroy; override;
    function Equals(Obj: TObject): Boolean; override;
    function GetHashCode: Int64; override;
  end;

  TDict = TObjectDictionary<TItem, TItem>;

constructor TItem.Create(const AName: string; AFails: Boolean);
begin
  Name := AName;
  Fails := AFails;
end;

destructor TItem.Destroy;
begin
  WriteLn('free ', Name);
  if Fails then
    raise Exception.Create(Name + ' failed');
  inherited Destroy;
end;

// Keys are equal by the first letter of the name, so that 'k1' and 'k1b' are the same key
function TItem.Equals(Obj: TObject): Boolean;
begin
  Result := (Obj is TItem) and (Copy(TItem(Obj).Name, 1, 2) = Copy(Name, 1, 2));
end;

function TItem.GetHashCode: Int64;
begin
  Result := KeyHash<string>(Copy(Name, 1, 2));
end;

procedure Report(E: Exception);
var
  S: Exception;
begin
  WriteLn(E.ClassName, ': ', E.Message);
  for S in E.Suppressed do
    WriteLn('  suppressed ', S.ClassName, ': ', S.Message);
end;

function Make(Ownerships: TDictionaryOwnerships; const Names: array of string): TDict;
var
  I: Integer;
begin
  Result := TDict.Create(Ownerships);
  for I := 0 to High(Names) do
    Result.Add(TItem.Create('k' + Names[I]), TItem.Create('v' + Names[I]));
end;

const
  Both = [TDictionaryOwnership.doOwnsKeys, TDictionaryOwnership.doOwnsValues];

var
  D: TDict;
  P: TDictionary<TItem, TItem>;
  K, V: TItem;
  Pair: TPair<TItem, TItem>;
  S: TObjectDictionary<string, TItem>;

begin
  WriteLn('-- removing');
  D := Make(Both, ['1', '2', '3']);
  K := TItem.Create('k2 probe');
  D.Remove(K);
  D.Remove(K);
  K.Free;
  WriteLn('count ', D.Count);
  WriteLn('-- clearing');
  D.Clear;
  WriteLn('count ', D.Count);
  D.Free;

  WriteLn('-- freeing');
  D := Make(Both, ['1', '2', '3']);
  D.Free;

  WriteLn('-- writing over');
  D := Make(Both, ['1', '2']);
  K := TItem.Create('k1 again');
  D.AddOrSetValue(K, TItem.Create('v1 new'));
  WriteLn('-- same key instance');
  for Pair in D do
    if Pair.Key.Name = 'k2' then
      K := Pair.Key;
  D[K] := TItem.Create('v2 new');
  WriteLn('-- same value instance');
  V := D[K];
  D[K] := V;
  for Pair in D do
    WriteLn(Pair.Key.Name, ' = ', Pair.Value.Name);
  D.Free;

  WriteLn('-- taking out');
  D := Make(Both, ['1', '2']);
  K := TItem.Create('k1 probe');
  Pair := D.ExtractPair(K);
  K.Free;
  WriteLn('extracted ', Pair.Key.Name, ' ', Pair.Value.Name);
  Pair.Key.Free;
  Pair.Value.Free;
  K := TItem.Create('k2 dup');
  V := TItem.Create('v2 dup');
  WriteLn('try add ', D.TryAdd(K, V));
  try
    D.Add(K, V);
  except
    on E: EListError do
      Report(E);
  end;
  K.Free;
  V.Free;
  D.Free;

  WriteLn('-- owning one side');
  D := Make([TDictionaryOwnership.doOwnsValues], ['1', '2']);
  K := nil;
  for Pair in D do
    if Pair.Key.Name = 'k1' then
      K := Pair.Key;
  V := TItem.Create('k1 again');
  D.AddOrSetValue(V, TItem.Create('v1 new'));
  V.Free;
  D.Remove(K);
  K.Free;
  WriteLn('-- keys left');
  for K in D.Keys do
    WriteLn(K.Name);
  K := D.Keys.ToArray[0];
  D.Free;
  K.Free;
  D := Make([TDictionaryOwnership.doOwnsKeys], ['1']);
  V := D.Values.ToArray[0];
  WriteLn('owns keys ', TDictionaryOwnership.doOwnsKeys in D.Ownerships, ', owns values ',
    TDictionaryOwnership.doOwnsValues in D.Ownerships);
  D.Free;
  V.Free;

  WriteLn('-- string keys');
  S := TObjectDictionary<string, TItem>.Create([TDictionaryOwnership.doOwnsValues]);
  S.Add('a', TItem.Create('a'));
  S['a'] := TItem.Create('a2');
  S.Add('b', TItem.Create('b'));
  S.Remove('a');
  S.Free;

  WriteLn('-- through the parent type');
  D := Make(Both, ['1', '2', '3']);
  P := D;
  K := TItem.Create('k1 probe');
  P.Remove(K);
  K.Free;
  P.AddOrSetValue(TItem.Create('k2 again'), TItem.Create('v2 new'));
  P.Clear;
  P.Free;

  WriteLn('-- a destructor raises');
  D := TDict.Create(Both);
  D.Add(TItem.Create('k1'), TItem.Create('v1', True));
  D.Add(TItem.Create('k2', True), TItem.Create('v2'));
  D.Add(TItem.Create('k3'), TItem.Create('v3'));
  try
    D.Clear;
  except
    on E: Exception do
      Report(E);
  end;
  WriteLn('count ', D.Count);
  D.Free;
end.
