// ===== TStringHelper (docs/spec/stdlib/System/TStringHelper.md) =====

// The names of the helper's own members (Insert, Trim, ...) come first in its bodies
// (docs/spec/objects.md 10.4), so the routines of the same names are written qualified.

function CharIn(C: Char; const AnyOf: array of Char): Boolean;
var
  I: Integer;
begin
  Result := False;
  for I := 0 to High(AnyOf) do
    if AnyOf[I] = C then
      Exit(True);
end;

function TStringHelper.GetChars(Index: Integer): Char;
begin
  if (Index < 0) or (Index >= System.Length(Self)) then
    raise ERangeError.Create('string index ' + IntToStr(Index) + ' is out of range 0..' +
      IntToStr(System.Length(Self) - 1));
  Result := Self[Index + 1];
end;

// The first Value in the range StartIndex..StartIndex + Count - 1 cut to the string, or -1.
function StrFind(const S, Value: string; StartIndex, Count: Integer): Integer;
var
  Len: Integer;
  Last: Int64;
  P: Integer;
begin
  Len := Length(S);
  Last := Int64(StartIndex) + Count;
  if Last > Len then
    Last := Len;
  if StartIndex < 0 then
    StartIndex := 0;
  if (Value = '') or (StartIndex >= Last) then
    Exit(-1);
  if (StartIndex = 0) and (Last = Len) then
    P := Pos(Value, S)
  else
    P := Pos(Value, Copy(S, StartIndex + 1, Last - StartIndex));
  if P = 0 then
    Result := -1
  else
    Result := StartIndex + P - 1;
end;

// The last Value that ends at or before StartIndex, or -1.
function StrFindLast(const S, Value: string; StartIndex: Integer): Integer;
var
  VLen, I, J: Integer;
begin
  Result := -1;
  VLen := Length(Value);
  if VLen = 0 then
    Exit;
  if StartIndex >= Length(S) then
    StartIndex := Length(S) - 1;
  // A negative start finds nothing; this also keeps StartIndex - VLen from overflowing
  if StartIndex < 0 then
    Exit;
  I := StartIndex - VLen + 1;
  while I >= 0 do
  begin
    J := 1;
    while (J <= VLen) and (S[I + J] = Value[J]) do
      Inc(J);
    if J > VLen then
      Exit(I);
    Dec(I);
  end;
end;

// The first (or, Backward, the last) character in AnyOf from StartIndex on (or back), or -1.
function StrFindAny(const S: string; const AnyOf: array of Char; StartIndex: Integer;
  Backward: Boolean): Integer;
var
  I: Integer;
begin
  Result := -1;
  if Backward then
  begin
    if StartIndex >= Length(S) then
      StartIndex := Length(S) - 1;
    for I := StartIndex downto 0 do
      if CharIn(S[I + 1], AnyOf) then
        Exit(I);
  end
  else
  begin
    if StartIndex < 0 then
      StartIndex := 0;
    for I := StartIndex to Length(S) - 1 do
      if CharIn(S[I + 1], AnyOf) then
        Exit(I);
  end;
end;

function StrPad(const S: string; TotalWidth: Integer; PaddingChar: Char; Left: Boolean): string;
begin
  if TotalWidth <= Length(S) then
    Exit(S);
  if Left then
    Result := StringOfChar(PaddingChar, TotalWidth - Length(S)) + S
  else
    Result := S + StringOfChar(PaddingChar, TotalWidth - Length(S));
end;

function StrTrimChars(const S: string; const TrimChars: array of Char; Left, Right: Boolean): string;
var
  First, Last: Integer;
begin
  First := 1;
  Last := Length(S);
  if Left then
    while (First <= Last) and CharIn(S[First], TrimChars) do
      Inc(First);
  if Right then
    while (Last >= First) and CharIn(S[Last], TrimChars) do
      Dec(Last);
  Result := Copy(S, First, Last - First + 1);
end;

class function TStringHelper.Compare(const StrA, StrB: string): Integer;
begin
  Result := CompareStr(StrA, StrB);
end;

class function TStringHelper.Compare(const StrA, StrB: string; IgnoreCase: Boolean): Integer;
begin
  if IgnoreCase then
    Result := AnsiCompareText(StrA, StrB)
  else
    Result := CompareStr(StrA, StrB);
end;

class function TStringHelper.IsNullOrEmpty(const Value: string): Boolean;
begin
  Result := Value = '';
end;

class function TStringHelper.Join(const Separator: string; const Values: array of string): string;
var
  I: Integer;
begin
  Result := '';
  for I := 0 to High(Values) do
  begin
    if I > 0 then
      Result := Result + Separator;
    Result := Result + Values[I];
  end;
end;

function TStringHelper.CompareTo(const StrB: string): Integer;
begin
  Result := CompareStr(Self, StrB);
end;

function TStringHelper.Contains(const Value: string): Boolean;
begin
  Result := Pos(Value, Self) > 0;
end;

function TStringHelper.CountChar(C: Char): Integer;
var
  I: Integer;
begin
  Result := 0;
  for I := 1 to System.Length(Self) do
    if Self[I] = C then
      Inc(Result);
end;

function TStringHelper.EndsWith(const Value: string): Boolean;
begin
  Result := EndsWith(Value, False);
end;

function TStringHelper.EndsWith(const Value: string; IgnoreCase: Boolean): Boolean;
var
  Tail: string;
begin
  if System.Length(Value) > System.Length(Self) then
    Exit(False);
  Tail := Copy(Self, System.Length(Self) - System.Length(Value) + 1, System.Length(Value));
  if IgnoreCase then
    Result := AnsiSameText(Tail, Value)
  else
    Result := Tail = Value;
end;

function TStringHelper.Equals(const Value: string): Boolean;
begin
  Result := Self = Value;
end;

function TStringHelper.IndexOf(const Value: string): Integer;
begin
  Result := Pos(Value, Self) - 1;
end;

function TStringHelper.IndexOf(const Value: string; StartIndex: Integer): Integer;
begin
  Result := StrFind(Self, Value, StartIndex, System.Length(Self));
end;

function TStringHelper.IndexOf(const Value: string; StartIndex, Count: Integer): Integer;
begin
  Result := StrFind(Self, Value, StartIndex, Count);
end;

function TStringHelper.IndexOfAny(const AnyOf: array of Char): Integer;
begin
  Result := StrFindAny(Self, AnyOf, 0, False);
end;

function TStringHelper.IndexOfAny(const AnyOf: array of Char; StartIndex: Integer): Integer;
begin
  Result := StrFindAny(Self, AnyOf, StartIndex, False);
end;

function TStringHelper.Insert(StartIndex: Integer; const Value: string): string;
begin
  Result := Self;
  if StartIndex < 0 then
    StartIndex := 0
  else if StartIndex > System.Length(Result) then
    StartIndex := System.Length(Result);
  System.Insert(Value, Result, StartIndex + 1);
end;

function TStringHelper.IsEmpty: Boolean;
begin
  Result := Self = '';
end;

function TStringHelper.LastIndexOf(const Value: string): Integer;
begin
  Result := StrFindLast(Self, Value, System.Length(Self) - 1);
end;

function TStringHelper.LastIndexOf(const Value: string; StartIndex: Integer): Integer;
begin
  Result := StrFindLast(Self, Value, StartIndex);
end;

function TStringHelper.LastIndexOfAny(const AnyOf: array of Char): Integer;
begin
  Result := StrFindAny(Self, AnyOf, System.Length(Self) - 1, True);
end;

function TStringHelper.LastIndexOfAny(const AnyOf: array of Char; StartIndex: Integer): Integer;
begin
  Result := StrFindAny(Self, AnyOf, StartIndex, True);
end;

function TStringHelper.PadLeft(TotalWidth: Integer): string;
begin
  Result := StrPad(Self, TotalWidth, ' ', True);
end;

function TStringHelper.PadLeft(TotalWidth: Integer; PaddingChar: Char): string;
begin
  Result := StrPad(Self, TotalWidth, PaddingChar, True);
end;

function TStringHelper.PadRight(TotalWidth: Integer): string;
begin
  Result := StrPad(Self, TotalWidth, ' ', False);
end;

function TStringHelper.PadRight(TotalWidth: Integer; PaddingChar: Char): string;
begin
  Result := StrPad(Self, TotalWidth, PaddingChar, False);
end;

function TStringHelper.QuotedString: string;
begin
  Result := QuotedString('''');
end;

function TStringHelper.QuotedString(QuoteChar: Char): string;
var
  I: Integer;
begin
  Result := QuoteChar;
  for I := 1 to System.Length(Self) do
  begin
    if Self[I] = QuoteChar then
      Result := Result + QuoteChar;
    Result := Result + Self[I];
  end;
  Result := Result + QuoteChar;
end;

function TStringHelper.Remove(StartIndex: Integer): string;
begin
  Result := Remove(StartIndex, System.Length(Self));
end;

function TStringHelper.Remove(StartIndex, Count: Integer): string;
begin
  Result := Self;
  if (StartIndex >= 0) and (StartIndex < System.Length(Result)) then
    System.Delete(Result, StartIndex + 1, Count);
end;

function TStringHelper.Replace(const OldValue, NewValue: string): string;
begin
  Result := StringReplace(Self, OldValue, NewValue, [TReplaceFlag.rfReplaceAll]);
end;

// Whether Sep occurs in S at the 1-based position At (compared in place, without a copy).
function SepAt(const S, Sep: string; At: Integer): Boolean;
var
  J: Integer;
begin
  if (Sep = '') or (Length(Sep) > Length(S) - At + 1) then
    Exit(False);
  for J := 1 to Length(Sep) do
    if S[At + J - 1] <> Sep[J] then
      Exit(False);
  Result := True;
end;

// The parts between the separators: one more than the separators found, empty ones included.
// At each position the first separator in the order given that matches there is taken. The
// array grows by doubling and is cut to the number of parts at the end.
function TStringHelper.Split(const Separators: array of string): TArray<string>;
var
  Len, Start, I, K, N, SepLen: Integer;
  C: Char;
begin
  Result := nil;
  SetLength(Result, 4);
  N := 0;
  Len := System.Length(Self);
  Start := 1;
  I := 1;
  while I <= Len do
  begin
    SepLen := 0;
    C := Self[I];
    for K := 0 to High(Separators) do
      // The first character is compared here, so that most positions make no call
      if (Separators[K] <> '') and (Separators[K][1] = C) and SepAt(Self, Separators[K], I) then
      begin
        SepLen := System.Length(Separators[K]);
        Break;
      end;
    if SepLen > 0 then
    begin
      if N = System.Length(Result) then
        SetLength(Result, N * 2);
      Result[N] := Copy(Self, Start, I - Start);
      Inc(N);
      I := I + SepLen;
      Start := I;
    end
    else
      Inc(I);
  end;
  if N = System.Length(Result) then
    SetLength(Result, N + 1);
  Result[N] := Copy(Self, Start, Len - Start + 1);
  SetLength(Result, N + 1);
end;

function TStringHelper.StartsWith(const Value: string): Boolean;
begin
  Result := StartsWith(Value, False);
end;

function TStringHelper.StartsWith(const Value: string; IgnoreCase: Boolean): Boolean;
var
  Head: string;
begin
  if System.Length(Value) > System.Length(Self) then
    Exit(False);
  Head := Copy(Self, 1, System.Length(Value));
  if IgnoreCase then
    Result := AnsiSameText(Head, Value)
  else
    Result := Head = Value;
end;

function TStringHelper.Substring(StartIndex: Integer): string;
begin
  Result := Substring(StartIndex, System.Length(Self));
end;

function TStringHelper.Substring(StartIndex, Length: Integer): string;
begin
  if StartIndex >= System.Length(Self) then
    Exit('');
  if StartIndex < 0 then
    StartIndex := 0;
  Result := Copy(Self, StartIndex + 1, Length);
end;

function TStringHelper.ToCharArray: TArray<Char>;
var
  I: Integer;
begin
  Result := nil;
  SetLength(Result, System.Length(Self));
  for I := 1 to System.Length(Self) do
    Result[I - 1] := Self[I];
end;

function TStringHelper.ToDouble: Double;
begin
  Result := StrToFloat(Self);
end;

function TStringHelper.ToInt64: Int64;
begin
  Result := StrToInt64(Self);
end;

function TStringHelper.ToInteger: Integer;
begin
  Result := StrToInt(Self);
end;

function TStringHelper.ToLower: string;
begin
  Result := AnsiLowerCase(Self);
end;

function TStringHelper.ToUpper: string;
begin
  Result := AnsiUpperCase(Self);
end;

function TStringHelper.Trim: string;
begin
  Result := System.SysUtils.Trim(Self);
end;

function TStringHelper.Trim(const TrimChars: array of Char): string;
begin
  Result := StrTrimChars(Self, TrimChars, True, True);
end;

function TStringHelper.TrimLeft: string;
begin
  Result := System.SysUtils.TrimLeft(Self);
end;

function TStringHelper.TrimLeft(const TrimChars: array of Char): string;
begin
  Result := StrTrimChars(Self, TrimChars, True, False);
end;

function TStringHelper.TrimRight: string;
begin
  Result := System.SysUtils.TrimRight(Self);
end;

function TStringHelper.TrimRight(const TrimChars: array of Char): string;
begin
  Result := StrTrimChars(Self, TrimChars, False, True);
end;
