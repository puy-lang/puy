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
