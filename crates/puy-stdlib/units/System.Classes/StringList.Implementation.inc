{ TStringsEnumerator }

function TStringsEnumerator.GetCurrent: string;
begin
  Result := FStrings.Get(FIndex);
end;

function TStringsEnumerator.MoveNext: Boolean;
begin
  Inc(FIndex);
  Result := FIndex < FStrings.GetCount;
end;

{ TStrings }

// Whether a string of DelimitedText is quoted: it has QuoteChar, Delimiter or a character up to
// a space (not StrictDelimiter) in it, or (StrictDelimiter) QuoteChar or Delimiter.
function StringsNeedsQuote(const S: string; Delimiter, QuoteChar: Char;
  Strict: Boolean): Boolean;
var
  I: Integer;
begin
  for I := 1 to Length(S) do
    if (S[I] = QuoteChar) or (S[I] = Delimiter) or (not Strict and (S[I] <= ' ')) then
      Exit(True);
  Result := False;
end;

// S between two QuoteChar, each QuoteChar in it written twice.
function StringsQuote(const S: string; QuoteChar: Char): string;
var
  I: Integer;
begin
  Result := QuoteChar;
  for I := 1 to Length(S) do
  begin
    if S[I] = QuoteChar then
      Result := Result + QuoteChar;
    Result := Result + S[I];
  end;
  Result := Result + QuoteChar;
end;

constructor TStrings.Create;
begin
  inherited Create;
  FDelimiter := ',';
  FQuoteChar := '"';
  FNameValueSeparator := '=';
  FStrictDelimiter := False;
  FLineBreak := sLineBreak;
end;

destructor TStrings.Destroy;
begin
  SetEncoding(nil);
  inherited Destroy;
end;

procedure TStrings.SetEncoding(Value: TEncoding);
begin
  // Value may be the copy the list owns (LoadFromStream(S, L.Encoding))
  if Value = FEncoding then
    Exit;
  if FOwnsEncoding then
    FEncoding.Free;
  FEncoding := nil;
  FOwnsEncoding := False;
  if Value = nil then
    Exit;
  if TEncoding.IsStandardEncoding(Value) then
    FEncoding := Value
  else
  begin
    FEncoding := Value.Clone;
    FOwnsEncoding := True;
  end;
end;

function TStrings.GetObject(Index: Integer): TObject;
begin
  Result := nil;
end;

// Keeps the object of the line.
procedure TStrings.Put(Index: Integer; const S: string);
begin
  var O := GetObject(Index);
  BeginUpdate;
  try
    Delete(Index);
    InsertObject(Index, S, O);
  finally
    EndUpdate;
  end;
end;

procedure TStrings.PutObject(Index: Integer; AObject: TObject);
begin
end;

function TStrings.GetCapacity: Integer;
begin
  Result := GetCount;
end;

procedure TStrings.SetCapacity(NewCapacity: Integer);
begin
end;

function TStrings.GetTextStr: string;
var
  I: Integer;
begin
  Result := '';
  for I := 0 to GetCount - 1 do
    Result := Result + Get(I) + FLineBreak;
end;

// Splits at LineBreak; when LineBreak is sLineBreak, at CR LF, LF and CR. A break at the end
// does not start another line.
procedure TStrings.SetTextStr(const Value: string);
var
  I, Start, N: Integer;
begin
  BeginUpdate;
  try
    Clear;
    N := Length(Value);
    Start := 1;
    I := 1;
    if FLineBreak = sLineBreak then
    begin
      while I <= N do
      begin
        if (Value[I] = #10) or (Value[I] = #13) then
        begin
          Add(Copy(Value, Start, I - Start));
          if (Value[I] = #13) and (I < N) and (Value[I + 1] = #10) then
            Inc(I);
          Start := I + 1;
        end;
        Inc(I);
      end;
    end
    else if FLineBreak <> '' then
    begin
      var B := Length(FLineBreak);
      while I <= N - B + 1 do
        if Copy(Value, I, B) = FLineBreak then
        begin
          Add(Copy(Value, Start, I - Start));
          I := I + B;
          Start := I;
        end
        else
          Inc(I);
    end;
    if Start <= N then
      Add(Copy(Value, Start, N - Start + 1));
  finally
    EndUpdate;
  end;
end;

procedure TStrings.SetUpdateState(Updating: Boolean);
begin
end;

function TStrings.CompareStrings(const S1, S2: string): Integer;
begin
  Result := AnsiCompareText(S1, S2);
end;

function TStrings.Add(const S: string): Integer;
begin
  Result := GetCount;
  Insert(Result, S);
end;

function TStrings.AddObject(const S: string; AObject: TObject): Integer;
begin
  Result := Add(S);
  PutObject(Result, AObject);
end;

function TStrings.AddPair(const Name, Value: string): TStrings;
begin
  Add(Name + FNameValueSeparator + Value);
  Result := Self;
end;

function TStrings.AddPair(const Name, Value: string; AObject: TObject): TStrings;
begin
  AddObject(Name + FNameValueSeparator + Value, AObject);
  Result := Self;
end;

procedure TStrings.Append(const S: string);
begin
  Add(S);
end;

procedure TStrings.AddStrings(const Strings: TStrings);
var
  I: Integer;
begin
  BeginUpdate;
  try
    for I := 0 to Strings.GetCount - 1 do
      AddObject(Strings.Get(I), Strings.GetObject(I));
  finally
    EndUpdate;
  end;
end;

procedure TStrings.AddStrings(const Strings: array of string);
var
  I: Integer;
begin
  BeginUpdate;
  try
    for I := 0 to High(Strings) do
      Add(Strings[I]);
  finally
    EndUpdate;
  end;
end;

procedure TStrings.Assign(const Source: TStrings);
begin
  if Source = Self then
    Exit;
  BeginUpdate;
  try
    Clear;
    FDelimiter := Source.FDelimiter;
    FQuoteChar := Source.FQuoteChar;
    FNameValueSeparator := Source.FNameValueSeparator;
    FStrictDelimiter := Source.FStrictDelimiter;
    FLineBreak := Source.FLineBreak;
    AddStrings(Source);
  finally
    EndUpdate;
  end;
end;

procedure TStrings.BeginUpdate;
begin
  if FUpdateCount = 0 then
    SetUpdateState(True);
  Inc(FUpdateCount);
end;

// An EndUpdate without a BeginUpdate does nothing.
procedure TStrings.EndUpdate;
begin
  if FUpdateCount = 0 then
    Exit;
  Dec(FUpdateCount);
  if FUpdateCount = 0 then
    SetUpdateState(False);
end;

procedure TStrings.Exchange(Index1, Index2: Integer);
begin
  var S := Get(Index1);
  var O := GetObject(Index1);
  BeginUpdate;
  try
    Put(Index1, Get(Index2));
    PutObject(Index1, GetObject(Index2));
    Put(Index2, S);
    PutObject(Index2, O);
  finally
    EndUpdate;
  end;
end;

function TStrings.GetEnumerator: TStringsEnumerator;
begin
  Result := Default(TStringsEnumerator);
  Result.FStrings := Self;
  Result.FIndex := -1;
end;

function TStrings.IndexOf(const S: string): Integer;
var
  I: Integer;
begin
  for I := 0 to GetCount - 1 do
    if CompareStrings(Get(I), S) = 0 then
      Exit(I);
  Result := -1;
end;

function TStrings.IndexOfName(const Name: string): Integer;
var
  I, P: Integer;
  S: string;
begin
  for I := 0 to GetCount - 1 do
  begin
    S := Get(I);
    P := Pos(FNameValueSeparator, S);
    if (P > 0) and (CompareStrings(Copy(S, 1, P - 1), Name) = 0) then
      Exit(I);
  end;
  Result := -1;
end;

function TStrings.IndexOfObject(AObject: TObject): Integer;
var
  I: Integer;
begin
  for I := 0 to GetCount - 1 do
    if GetObject(I) = AObject then
      Exit(I);
  Result := -1;
end;

procedure TStrings.InsertObject(Index: Integer; const S: string; AObject: TObject);
begin
  Insert(Index, S);
  PutObject(Index, AObject);
end;

procedure TStrings.LoadFromFile(const FileName: string);
begin
  LoadFromFile(FileName, nil);
end;

procedure TStrings.LoadFromFile(const FileName: string; Encoding: TEncoding);
begin
  LoadFromStream(TFileStream.Create(FileName, fmOpenRead), Encoding);
end;

procedure TStrings.LoadFromStream(const Stream: TStream);
begin
  LoadFromStream(Stream, nil);
end;

// Reads from the position to the end. Without Encoding, the BOM decides it (UTF-8, UTF-16LE,
// UTF-16BE), else TEncoding.Default; a preamble of the encoding is skipped.
procedure TStrings.LoadFromStream(const Stream: TStream; Encoding: TEncoding);
var
  Buffer: TBytes;
begin
  var N := Stream.Size - Stream.Position;
  if N > MaxInt then
    raise EOutOfMemory.Create('the ' + IntToStr(N) + ' bytes of the stream do not fit in ' +
      'one TBytes');
  SetLength(Buffer, Integer(N));
  if N > 0 then
    Stream.ReadBuffer(Buffer, Integer(N));
  var Skip := TEncoding.GetBufferEncoding(Buffer, Encoding);
  var S := Encoding.GetString(Buffer, Skip, Length(Buffer) - Skip);
  BeginUpdate;
  try
    SetEncoding(Encoding);
    SetTextStr(S);
  finally
    EndUpdate;
  end;
end;

procedure TStrings.Move(CurIndex, NewIndex: Integer);
begin
  if CurIndex = NewIndex then
    Exit;
  var S := Get(CurIndex);
  var O := GetObject(CurIndex);
  BeginUpdate;
  try
    Delete(CurIndex);
    InsertObject(NewIndex, S, O);
  finally
    EndUpdate;
  end;
end;

procedure TStrings.SaveToFile(const FileName: string);
begin
  SaveToFile(FileName, nil);
end;

procedure TStrings.SaveToFile(const FileName: string; Encoding: TEncoding);
begin
  var F := TFileStream.Create(FileName, fmCreate);
  SaveToStream(F, Encoding);
  F.Close;
end;

procedure TStrings.SaveToStream(const Stream: TStream);
begin
  SaveToStream(Stream, nil);
end;

// Writes the preamble of the encoding, then Text. Without Encoding, the encoding of the last
// LoadFromStream, else TEncoding.Default (no BOM).
procedure TStrings.SaveToStream(const Stream: TStream; Encoding: TEncoding);
var
  Bytes: TBytes;
begin
  if Encoding = nil then
    Encoding := FEncoding;
  if Encoding = nil then
    Encoding := TEncoding.Default;
  Bytes := Encoding.GetPreamble;
  if Length(Bytes) > 0 then
    Stream.WriteBuffer(Bytes, Length(Bytes));
  Bytes := Encoding.GetBytes(GetTextStr);
  if Length(Bytes) > 0 then
    Stream.WriteBuffer(Bytes, Length(Bytes));
end;

function TStrings.ToStringArray: TArray<string>;
var
  I: Integer;
begin
  SetLength(Result, GetCount);
  for I := 0 to High(Result) do
    Result[I] := Get(I);
end;

function TStrings.ToObjectArray: TArray<TObject>;
var
  I: Integer;
begin
  SetLength(Result, GetCount);
  for I := 0 to High(Result) do
    Result[I] := GetObject(I);
end;

function TStrings.GetRange(const Range: TIndexRange): TArray<string>;
var
  AFirst, ALast, I: Integer;
begin
  Range.Clamp(0, GetCount - 1, AFirst, ALast);
  SetLength(Result, ALast - AFirst + 1);
  for I := 0 to High(Result) do
    Result[I] := Get(AFirst + I);
end;

// With Delimiter ',', QuoteChar '"' and StrictDelimiter False, whatever the properties are.
function TStrings.GetCommaText: string;
begin
  var D := FDelimiter;
  var Q := FQuoteChar;
  var Strict := FStrictDelimiter;
  FDelimiter := ',';
  FQuoteChar := '"';
  FStrictDelimiter := False;
  try
    Result := GetDelimitedText;
  finally
    FDelimiter := D;
    FQuoteChar := Q;
    FStrictDelimiter := Strict;
  end;
end;

procedure TStrings.SetCommaText(const Value: string);
begin
  var D := FDelimiter;
  var Q := FQuoteChar;
  var Strict := FStrictDelimiter;
  FDelimiter := ',';
  FQuoteChar := '"';
  FStrictDelimiter := False;
  try
    SetDelimitedText(Value);
  finally
    FDelimiter := D;
    FQuoteChar := Q;
    FStrictDelimiter := Strict;
  end;
end;

function TStrings.GetDelimitedText: string;
var
  I: Integer;
  S: string;
begin
  var N := GetCount;
  if (N = 1) and (Get(0) = '') then
    Exit(FQuoteChar + FQuoteChar);
  Result := '';
  for I := 0 to N - 1 do
  begin
    S := Get(I);
    if StringsNeedsQuote(S, FDelimiter, FQuoteChar, FStrictDelimiter) then
      S := StringsQuote(S, FQuoteChar);
    if I > 0 then
      Result := Result + FDelimiter;
    Result := Result + S;
  end;
end;

// Not StrictDelimiter: characters up to a space separate strings as Delimiter does, and are
// skipped around them. A string that starts with QuoteChar runs to the next lone QuoteChar (a
// doubled one is one QuoteChar in the string). A Delimiter that is the last character adds an
// empty string; one followed by spaces only does not.
procedure TStrings.SetDelimitedText(const Value: string);
var
  I, N: Integer;
  S: string;
begin
  N := Length(Value);
  I := 1;
  BeginUpdate;
  try
    Clear;
    if not FStrictDelimiter then
      while (I <= N) and (Value[I] <= ' ') do
        Inc(I);
    while I <= N do
    begin
      S := '';
      if Value[I] = FQuoteChar then
      begin
        Inc(I);
        while I <= N do
        begin
          if Value[I] = FQuoteChar then
          begin
            if (I < N) and (Value[I + 1] = FQuoteChar) then
              Inc(I)
            else
            begin
              Inc(I);
              Break;
            end;
          end;
          S := S + Value[I];
          Inc(I);
        end;
      end
      else
        while (I <= N) and (Value[I] <> FDelimiter) and
          (FStrictDelimiter or (Value[I] > ' ')) do
        begin
          S := S + Value[I];
          Inc(I);
        end;
      Add(S);
      if not FStrictDelimiter then
        while (I <= N) and (Value[I] <= ' ') do
          Inc(I);
      if (I <= N) and (Value[I] = FDelimiter) then
      begin
        // Only a Delimiter that is the last character adds an empty string (as Delphi)
        if I = N then
          Add('');
        Inc(I);
        if not FStrictDelimiter then
          while (I <= N) and (Value[I] <= ' ') do
            Inc(I);
      end;
    end;
  finally
    EndUpdate;
  end;
end;

// The part before the first NameValueSeparator; '' when there is none.
function TStrings.GetName(Index: Integer): string;
begin
  var S := Get(Index);
  var P := Pos(FNameValueSeparator, S);
  if P > 0 then
    Result := Copy(S, 1, P - 1)
  else
    Result := '';
end;

function TStrings.GetValue(const Name: string): string;
begin
  var I := IndexOfName(Name);
  if I >= 0 then
    Result := GetValueFromIndex(I)
  else
    Result := '';
end;

// '' removes the line of Name; else the line becomes Name, NameValueSeparator and Value (Name as
// written here), or is added.
procedure TStrings.SetValue(const Name, Value: string);
begin
  var I := IndexOfName(Name);
  if Value = '' then
  begin
    if I >= 0 then
      Delete(I);
    Exit;
  end;
  if I < 0 then
    Add(Name + FNameValueSeparator + Value)
  else
    Put(I, Name + FNameValueSeparator + Value);
end;

// The part after the first NameValueSeparator; '' when there is none.
function TStrings.GetValueFromIndex(Index: Integer): string;
begin
  var S := Get(Index);
  var P := Pos(FNameValueSeparator, S);
  if P > 0 then
    Result := Copy(S, P + 1, Length(S) - P)
  else
    Result := '';
end;

// '' removes the line; else the line becomes Names[Index], NameValueSeparator and Value (a line
// without NameValueSeparator has the name '', so it becomes `=Value`).
procedure TStrings.SetValueFromIndex(Index: Integer; const Value: string);
begin
  if Value = '' then
    Delete(Index)
  else
    Put(Index, GetName(Index) + FNameValueSeparator + Value);
end;

{ TStringList }

// The exception for an instance of a reference counted class put in a list that owns its
// objects (as the owning containers; docs/spec/collections.md 9.2).
function StringListOwnError(const ClassName: string): EInvalidOperation;
begin
  Result := EInvalidOperation.Create('cannot own a reference counted object of class `' +
    ClassName + '`');
end;

function StringListCompareStrings(List: TStringList; Index1, Index2: Integer): Integer;
begin
  Result := List.CompareStrings(List.FList[Index1].FString, List.FList[Index2].FString);
end;

constructor TStringList.Create;
begin
  inherited Create;
  FList := TList<TStringItem>.Create;
  FDuplicates := TDuplicates.dupIgnore;
end;

constructor TStringList.Create(OwnsObjects: Boolean);
begin
  inherited Create;
  FList := TList<TStringItem>.Create;
  FDuplicates := TDuplicates.dupIgnore;
  FOwnsObjects := OwnsObjects;
end;

destructor TStringList.Destroy;
begin
  FOnChange := nil;
  FOnChanging := nil;
  try
    if FOwnsObjects and (FList.Count > 0) then
    begin
      var Items := FList.ToArray;
      FList.Clear;
      FreeObjects(Items, 0);
    end;
  finally
    FList.Free;
    inherited Destroy;
  end;
end;

procedure TStringList.FreeObjects(const Items: TArray<TStringItem>; From: Integer);
var
  I: Integer;
  Done: Boolean;
begin
  for I := From to High(Items) do
  begin
    Done := False;
    try
      Items[I].FObject.Free;
      Done := True;
    finally
      // An exception raised by the rest overlaps this one (Suppressed)
      if not Done then
        FreeObjects(Items, I + 1);
    end;
  end;
end;

procedure TStringList.CheckOwnable(AObject: TObject);
begin
  if FOwnsObjects and IsRefCountedObject(AObject) then
    raise StringListOwnError(AObject.ClassName);
end;

// The exception of an index outside 0..Max, as TList<T> (docs/spec/collections.md 3.2).
procedure TStringList.CheckIndex(Index, Max: Integer);
begin
  if (Index < 0) or (Index > Max) then
    raise ERangeError.Create('list index ' + IntToStr(Index) + ' is out of range 0..' +
      IntToStr(Max));
end;

procedure TStringList.CheckUnsorted(const What: string);
begin
  if FSorted then
    raise EStringListError.Create('cannot ' + What + ' a sorted list');
end;

function TStringList.Get(Index: Integer): string;
begin
  Result := FList[Index].FString;
end;

function TStringList.GetCount: Integer;
begin
  Result := FList.Count;
end;

function TStringList.GetObject(Index: Integer): TObject;
begin
  Result := FList[Index].FObject;
end;

procedure TStringList.Put(Index: Integer; const S: string);
begin
  var Item := FList[Index];
  CheckUnsorted('change a string of');
  Changing;
  Item.FString := S;
  FList[Index] := Item;
  Changed;
end;

// The object written over is not freed, even when the list owns its objects.
procedure TStringList.PutObject(Index: Integer; AObject: TObject);
begin
  var Item := FList[Index];
  CheckOwnable(AObject);
  Changing;
  Item.FObject := AObject;
  FList[Index] := Item;
  Changed;
end;

function TStringList.GetCapacity: Integer;
begin
  Result := FList.Capacity;
end;

procedure TStringList.SetCapacity(NewCapacity: Integer);
begin
  FList.Capacity := NewCapacity;
end;

procedure TStringList.SetUpdateState(Updating: Boolean);
begin
  if Updating then
    Changing
  else
    Changed;
end;

procedure TStringList.Changing;
begin
  if (UpdateCount = 0) and Assigned(FOnChanging) then
    FOnChanging(Self);
end;

procedure TStringList.Changed;
begin
  if (UpdateCount = 0) and Assigned(FOnChange) then
    FOnChange(Self);
end;

function TStringList.CompareStrings(const S1, S2: string): Integer;
begin
  if FCaseSensitive then
    Result := CompareStr(S1, S2)
  else
    Result := AnsiCompareText(S1, S2);
end;

function TStringList.Add(const S: string): Integer;
begin
  Result := AddObject(S, nil);
end;

function TStringList.AddObject(const S: string; AObject: TObject): Integer;
begin
  Result := FList.Count;
  if FSorted and Find(S, Result) then
    case FDuplicates of
      TDuplicates.dupIgnore:
        Exit;
      TDuplicates.dupError:
        raise EStringListError.Create('the sorted list already has `' + S + '`');
    end;
  InsertItem(Result, S, AObject);
end;

procedure TStringList.InsertItem(Index: Integer; const S: string; AObject: TObject);
var
  Item: TStringItem;
begin
  CheckIndex(Index, FList.Count);
  CheckOwnable(AObject);
  Item := Default(TStringItem);
  Item.FString := S;
  Item.FObject := AObject;
  Changing;
  FList.Insert(Index, Item);
  Changed;
end;

procedure TStringList.Clear;
begin
  if FList.Count = 0 then
    Exit;
  Changing;
  var Items := FList.ToArray;
  FList.Clear;
  try
    if FOwnsObjects then
      FreeObjects(Items, 0);
  finally
    Changed;
  end;
end;

procedure TStringList.Delete(Index: Integer);
begin
  var Item := FList[Index];
  Changing;
  FList.Delete(Index);
  try
    if FOwnsObjects then
      Item.FObject.Free;
  finally
    Changed;
  end;
end;

procedure TStringList.Exchange(Index1, Index2: Integer);
begin
  CheckIndex(Index1, FList.Count - 1);
  CheckIndex(Index2, FList.Count - 1);
  CheckUnsorted('reorder');
  Changing;
  FList.Exchange(Index1, Index2);
  Changed;
end;

function TStringList.Find(const S: string; var Index: Integer): Boolean;
var
  L, H, M, C: Integer;
begin
  Result := False;
  L := 0;
  H := FList.Count - 1;
  while L <= H do
  begin
    M := (L + H) div 2;
    C := CompareStrings(FList[M].FString, S);
    if C < 0 then
      L := M + 1
    else
    begin
      H := M - 1;
      if C = 0 then
        Result := True;
    end;
  end;
  Index := L;
end;

function TStringList.IndexOf(const S: string): Integer;
begin
  if not FSorted then
    Exit(inherited IndexOf(S));
  Result := 0;
  if not Find(S, Result) then
    Result := -1;
end;

procedure TStringList.Insert(Index: Integer; const S: string);
begin
  InsertObject(Index, S, nil);
end;

procedure TStringList.InsertObject(Index: Integer; const S: string; AObject: TObject);
begin
  CheckUnsorted('insert into');
  InsertItem(Index, S, AObject);
end;

// Moves the line with its object; the object is not freed.
procedure TStringList.Move(CurIndex, NewIndex: Integer);
begin
  CheckIndex(CurIndex, FList.Count - 1);
  CheckIndex(NewIndex, FList.Count - 1);
  CheckUnsorted('reorder');
  if CurIndex = NewIndex then
    Exit;
  Changing;
  FList.Move(CurIndex, NewIndex);
  Changed;
end;

procedure TStringList.Sort;
begin
  CustomSort(StringListCompareStrings);
end;

procedure TStringList.CustomSort(Compare: TStringListSortCompare);
begin
  if not FSorted then
    SortBy(Compare);
end;

// Sorts the indexes stably and then puts the lines in that order: Compare sees the list as it
// was while it sorts. A Compare that changes the count gets EInvalidOperation, and the list
// keeps its change. Changed is called even when Compare raises.
procedure TStringList.SortBy(Compare: TStringListSortCompare);
var
  Order: TArray<Integer>;
  Items: TArray<TStringItem>;
  I: Integer;
begin
  var N := FList.Count;
  if N < 2 then
    Exit;
  SetLength(Order, N);
  for I := 0 to N - 1 do
    Order[I] := I;
  Changing;
  try
    var List := Self;
    TArray.Sort<Integer>(Order, TComparer<Integer>.Construct(
      function(const Left, Right: Integer): Integer
      begin
        Result := Compare(List, Left, Right);
      end));
    if FList.Count <> N then
      raise EInvalidOperation.Create('the list was changed while it was being sorted');
    Items := FList.ToArray;
    for I := 0 to N - 1 do
      FList[I] := Items[Order[I]];
  finally
    Changed;
  end;
end;

procedure TStringList.SetSorted(Value: Boolean);
begin
  if Value = FSorted then
    Exit;
  if Value then
    SortBy(StringListCompareStrings);
  FSorted := Value;
end;

procedure TStringList.SetCaseSensitive(Value: Boolean);
begin
  if Value = FCaseSensitive then
    Exit;
  FCaseSensitive := Value;
  if FSorted then
    SortBy(StringListCompareStrings);
end;

// Turning it on checks the objects already in the list, and leaves it off when one of them
// cannot be owned.
procedure TStringList.SetOwnsObjects(Value: Boolean);
var
  I: Integer;
begin
  if Value and not FOwnsObjects then
    for I := 0 to FList.Count - 1 do
      if IsRefCountedObject(FList[I].FObject) then
        raise StringListOwnError(FList[I].FObject.ClassName);
  FOwnsObjects := Value;
end;
