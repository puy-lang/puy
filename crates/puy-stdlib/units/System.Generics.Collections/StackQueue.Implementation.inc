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
