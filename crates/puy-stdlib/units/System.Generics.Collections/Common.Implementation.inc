// ===== Common (chapter 4) =====

function TEnumerable<T>.TArrayEnumerator.GetCurrent: T;
begin
  Result := FItems[FIndex];
end;

function TEnumerable<T>.TArrayEnumerator.MoveNext: Boolean;
begin
  Inc(FIndex);
  Result := FIndex < Length(FItems);
end;

function TEnumerable<T>.GetEnumerator: TArrayEnumerator;
begin
  Result := Default(TArrayEnumerator);
  Result.FItems := ToArray;
  Result.FIndex := -1;
end;

constructor TPair<K, V>.Create(const AKey: K; const AValue: V);
begin
  Key := AKey;
  Value := AValue;
end;

// The capacity after Current when one more item is needed: doubled, and at least 4 (3.5).
function GrownCapacity(Current: Integer): Integer;
begin
  if Current < 2 then
    Result := 4
  else if Current > MaxInt div 2 then
    Result := MaxInt
  else
    Result := Current * 2;
end;

// Whether RemoveAll (TList<T>, TStack<T>, TQueue<T>) lets go of the whole array rather than
// writing Default(T) to each of the Count slots in use: always when the container is being freed
// (not KeepCapacity), and otherwise unless only a few of the Capacity slots are in use, where
// making a new array of the same capacity would cost more than the writes.
function ReleasesWholeArray(KeepCapacity: Boolean; Count, Capacity: Integer): Boolean;
begin
  Result := not KeepCapacity or (Count > Capacity div 4);
end;

// The exception for a capacity below the count (3.2). Kind is `stack`, `queue`, ...
function CapacityError(const Kind: string; Value, Count: Integer): ERangeError;
begin
  Result := ERangeError.Create(Format('%s capacity %d is less than the count %d',
    [Kind, Value, Count]));
end;

// The exception for a change during enumeration (3.3).
function ModifiedError(const Container: TObject): EInvalidOperation;
begin
  Result := EInvalidOperation.Create('`' + Container.ClassName +
    '` was modified during enumeration');
end;

// The change number after Version (3.3). Wraps round to 0 after MaxInt instead of overflowing
// under {$Q+}: an enumerator only compares the number for equality. The most frequent changes
// (TList<T>.Add, adding to and removing from a dictionary) write the same rule out in place, to
// save the call; TDictionary<K, V>.Append writes it as (Version + 1) and MaxInt under {$Q-},
// which gives the same number. A change of the rule changes those places too.
function NextVersion(Version: Integer): Integer;
begin
  if Version = MaxInt then
    Result := 0
  else
    Result := Version + 1;
end;

const
  // Whether a container calls its Notify (KeyNotify, ValueNotify): not known until there is
  // first something to tell, then whether the class of the container overrides it (12.1).
  NOTIFY_UNKNOWN = 0;
  NOTIFY_CALL = 1;
  NOTIFY_SKIP = 2;

// The NOTIFY_* for a method that is overridden or not.
function NotifyState(Overridden: Boolean): Integer;
begin
  if Overridden then
    Result := NOTIFY_CALL
  else
    Result := NOTIFY_SKIP;
end;
