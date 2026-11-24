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
