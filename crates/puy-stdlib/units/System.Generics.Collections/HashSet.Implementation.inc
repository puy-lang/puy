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
