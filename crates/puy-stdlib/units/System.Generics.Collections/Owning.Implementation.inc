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
